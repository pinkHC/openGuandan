use std::{
    collections::HashMap,
    convert::Infallible,
    net::IpAddr,
    sync::Arc,
    time::{Duration, Instant},
};

use axum::{
    Router,
    body::Body,
    extract::{DefaultBodyLimit, Request},
    http::{
        HeaderValue, Method, StatusCode,
        header::{CACHE_CONTROL, RETRY_AFTER},
    },
    middleware::{self, Next},
    response::Response,
};
use parking_lot::Mutex;
use serde_json::json;
use socketioxide::{SocketIo, SocketIoBuilder};
use tokio::task::JoinHandle;
use tower_http::{
    cors::{AllowOrigin, Any, CorsLayer},
    trace::TraceLayer,
};

use crate::{
    config::ServerConfig,
    rooms::room_service::RoomService,
    transport::{client_ip::client_ip, http, websocket},
};

const MAX_BODY_BYTES: usize = 64 * 1024;
const HTTP_RATE_WINDOW: Duration = Duration::from_secs(60);
const HTTP_RATE_MAX: u32 = 120;

pub struct Application {
    pub router: Router,
    pub io: SocketIo,
    pub rooms: RoomService,
    cleanup_task: JoinHandle<()>,
    publication_task: JoinHandle<()>,
}

impl Application {
    pub async fn shutdown(self) {
        self.cleanup_task.abort();
        self.io.close().await;
        self.publication_task.abort();
    }
}

#[derive(Debug, thiserror::Error)]
pub enum AppBuildError {
    #[error("invalid CORS origin header value: {0}")]
    InvalidCorsOrigin(String),
}

#[derive(Clone)]
struct HttpRateLimiter {
    windows: Arc<Mutex<HashMap<Option<IpAddr>, HttpRateWindow>>>,
    trust_proxy: bool,
}

impl Default for HttpRateLimiter {
    fn default() -> Self {
        Self {
            windows: Arc::new(Mutex::new(HashMap::new())),
            trust_proxy: false,
        }
    }
}

#[derive(Clone, Copy)]
struct HttpRateWindow {
    started_at: Instant,
    count: u32,
}

pub async fn build_application(config: &ServerConfig) -> Result<Application, AppBuildError> {
    let rooms = RoomService::new(config.reconnect_grace_ms, config.room_idle_ttl_ms);
    let (socket_layer, io) = SocketIoBuilder::new()
        .max_payload(MAX_BODY_BYTES as u64)
        .build_layer();
    let publication_task = websocket::attach(&io, rooms.clone(), config.trust_proxy);

    let cors = cors_layer(&config.cors_origins)?;
    let http_rate_limiter = HttpRateLimiter {
        trust_proxy: config.trust_proxy,
        ..HttpRateLimiter::default()
    };
    let engine_handshake_limiter = HttpRateLimiter {
        trust_proxy: config.trust_proxy,
        ..HttpRateLimiter::default()
    };
    let router = http::routes(rooms.clone())
        .layer(DefaultBodyLimit::max(MAX_BODY_BYTES))
        .layer(middleware::from_fn_with_state(
            http_rate_limiter,
            enforce_http_rate_limit,
        ))
        .layer(middleware::from_fn(add_no_store_header))
        // Socket.IO must wrap the HTTP router, while CORS must wrap both.
        .layer(socket_layer)
        // Engine.IO opens are intercepted by the Socket.IO layer, so this gate
        // must sit outside it. Established polling requests carry `sid` and do
        // not consume additional handshake quota.
        .layer(middleware::from_fn_with_state(
            engine_handshake_limiter,
            enforce_engine_handshake_rate_limit,
        ))
        .layer(cors)
        .layer(TraceLayer::new_for_http());

    let cleanup_rooms = rooms.clone();
    let cleanup_task = tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(60));
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        // Tokio intervals tick immediately once; cleanup in the TypeScript server
        // first ran after one complete minute.
        interval.tick().await;
        loop {
            interval.tick().await;
            cleanup_rooms.remove_expired().await;
        }
    });

    Ok(Application {
        router,
        io,
        rooms,
        cleanup_task,
        publication_task,
    })
}

fn cors_layer(origins: &[String]) -> Result<CorsLayer, AppBuildError> {
    let origins = origins
        .iter()
        .map(|origin| {
            HeaderValue::from_str(origin)
                .map_err(|_| AppBuildError::InvalidCorsOrigin(origin.clone()))
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(CorsLayer::new()
        .allow_origin(AllowOrigin::list(origins))
        .allow_methods([Method::GET, Method::POST])
        .allow_headers(Any))
}

async fn add_no_store_header(request: Request, next: Next) -> Response {
    let mut response = next.run(request).await;
    response
        .headers_mut()
        .insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}

async fn enforce_http_rate_limit(
    axum::extract::State(limiter): axum::extract::State<HttpRateLimiter>,
    request: Request,
    next: Next,
) -> Result<Response, Infallible> {
    if consume_rate_limit(&limiter, &request) {
        return Ok(rate_limited_response());
    }
    Ok(next.run(request).await)
}

async fn enforce_engine_handshake_rate_limit(
    axum::extract::State(limiter): axum::extract::State<HttpRateLimiter>,
    request: Request,
    next: Next,
) -> Result<Response, Infallible> {
    if is_initial_engine_handshake(&request) && consume_rate_limit(&limiter, &request) {
        return Ok(rate_limited_response());
    }
    Ok(next.run(request).await)
}

fn is_initial_engine_handshake(request: &Request) -> bool {
    request.uri().path().starts_with("/socket.io")
        && !request.uri().query().is_some_and(has_valid_engine_sid)
}

fn has_valid_engine_sid(query: &str) -> bool {
    query
        .split('&')
        .find(|pair| pair.starts_with("sid="))
        .and_then(|pair| pair.split('=').nth(1))
        .is_some_and(|sid| {
            sid.len() == 16
                && sid
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
        })
}

fn consume_rate_limit(limiter: &HttpRateLimiter, request: &Request) -> bool {
    let key = client_ip(request.headers(), request.extensions(), limiter.trust_proxy);
    let now = Instant::now();
    {
        let mut windows = limiter.windows.lock();
        let count = {
            let window = windows.entry(key).or_insert(HttpRateWindow {
                started_at: now,
                count: 0,
            });
            if now.duration_since(window.started_at) >= HTTP_RATE_WINDOW {
                *window = HttpRateWindow {
                    started_at: now,
                    count: 0,
                };
            }
            window.count = window.count.saturating_add(1);
            window.count
        };

        if windows.len() > 4_096 {
            windows
                .retain(|_, candidate| now.duration_since(candidate.started_at) < HTTP_RATE_WINDOW);
        }
        count > HTTP_RATE_MAX
    }
}

fn rate_limited_response() -> Response {
    let mut response = Response::new(Body::from(
        serde_json::to_vec(&json!({
            "error": {
                "code": "RATE_LIMITED",
                "message": "操作过于频繁，请稍后重试",
                "details": null
            }
        }))
        .expect("static rate-limit response must serialize"),
    ));
    *response.status_mut() = StatusCode::TOO_MANY_REQUESTS;
    response.headers_mut().insert(
        axum::http::header::CONTENT_TYPE,
        HeaderValue::from_static("application/json"),
    );
    response
        .headers_mut()
        .insert(RETRY_AFTER, HeaderValue::from_static("60"));
    response
}

#[cfg(test)]
mod tests {
    use axum::extract::ConnectInfo;
    use std::net::SocketAddr;
    use tower::ServiceExt;

    use super::*;

    fn request(uri: &str) -> Request {
        Request::builder().uri(uri).body(Body::empty()).unwrap()
    }

    #[test]
    fn engine_gate_matches_socketioxide_prefix_and_only_initial_opens() {
        assert!(is_initial_engine_handshake(&request(
            "/socket.io/?EIO=4&transport=websocket"
        )));
        assert!(is_initial_engine_handshake(&request(
            "/socket.io/anything?EIO=4&transport=polling"
        )));
        assert!(is_initial_engine_handshake(&request(
            "/socket.ioevil?EIO=4&transport=websocket"
        )));
        assert!(!is_initial_engine_handshake(&request(
            "/socket.io/?EIO=4&transport=polling&sid=AAAAAAAAAAAAAAHs"
        )));
        assert!(is_initial_engine_handshake(&request(
            "/socket.io/?EIO=4&transport=websocket&sid=!"
        )));
        assert!(is_initial_engine_handshake(&request(
            "/socket.io/?EIO=4&transport=websocket&sid="
        )));
        assert!(!is_initial_engine_handshake(&request("/api/rooms")));
    }

    #[tokio::test]
    async fn engine_gate_limits_opens_but_not_established_polling() {
        let app = Router::new()
            .fallback(|| async { StatusCode::NO_CONTENT })
            .layer(middleware::from_fn_with_state(
                HttpRateLimiter::default(),
                enforce_engine_handshake_rate_limit,
            ));
        let engine_request = |uri: &str| {
            let mut request = request(uri);
            request
                .extensions_mut()
                .insert(ConnectInfo(SocketAddr::from(([127, 0, 0, 1], 30_004))));
            request
        };

        for _ in 0..HTTP_RATE_MAX {
            let response = app
                .clone()
                .oneshot(engine_request("/socket.io/?EIO=4&transport=polling"))
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::NO_CONTENT);
        }
        let established = app
            .clone()
            .oneshot(engine_request(
                "/socket.io/?EIO=4&transport=polling&sid=AAAAAAAAAAAAAAHs",
            ))
            .await
            .unwrap();
        assert_eq!(established.status(), StatusCode::NO_CONTENT);

        let limited = app
            .oneshot(engine_request("/socket.io/?EIO=4&transport=websocket"))
            .await
            .unwrap();
        assert_eq!(limited.status(), StatusCode::TOO_MANY_REQUESTS);
    }
}
