use std::{
    collections::HashMap,
    net::IpAddr,
    sync::Arc,
    time::{Duration, Instant},
};

use axum::{
    Json, Router,
    extract::{DefaultBodyLimit, Request},
    http::{
        HeaderValue, Method, StatusCode,
        header::{CACHE_CONTROL, RETRY_AFTER},
    },
    middleware::{self, Next},
    response::{IntoResponse, Response},
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
    transport::{client_ip::client_ip, http, rate_limit::RateWindow, websocket},
};

const MAX_BODY_BYTES: usize = 64 * 1024;
const HTTP_RATE_WINDOW: Duration = Duration::from_secs(60);
const HTTP_RATE_MAX: u32 = 120;

pub struct Application {
    pub router: Router,
    pub io: SocketIo,
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

#[derive(Clone, Default)]
struct HttpRateLimiter {
    windows: Arc<Mutex<HashMap<Option<IpAddr>, RateWindow>>>,
    trust_proxy: bool,
    initial_engine_handshakes_only: bool,
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
        initial_engine_handshakes_only: true,
        ..HttpRateLimiter::default()
    };
    let router = http::routes(rooms.clone())
        .layer(DefaultBodyLimit::max(MAX_BODY_BYTES))
        .layer(middleware::from_fn_with_state(
            http_rate_limiter,
            enforce_rate_limit,
        ))
        .layer(middleware::from_fn(add_no_store_header))
        // Socket.IO must wrap the HTTP router, while CORS must wrap both.
        .layer(socket_layer)
        // Engine.IO opens are intercepted by the Socket.IO layer, so this gate
        // must sit outside it. Established polling requests carry `sid` and do
        // not consume additional handshake quota.
        .layer(middleware::from_fn_with_state(
            engine_handshake_limiter,
            enforce_rate_limit,
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

async fn enforce_rate_limit(
    axum::extract::State(limiter): axum::extract::State<HttpRateLimiter>,
    request: Request,
    next: Next,
) -> Response {
    if (!limiter.initial_engine_handshakes_only || is_initial_engine_handshake(&request))
        && consume_rate_limit(&limiter, &request)
    {
        return rate_limited_response();
    }
    next.run(request).await
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
    let mut windows = limiter.windows.lock();
    let limited = windows
        .entry(key)
        .or_insert_with(|| RateWindow::new(now))
        .consume(now, HTTP_RATE_WINDOW, HTTP_RATE_MAX);
    if windows.len() > 4_096 {
        windows.retain(|_, window| !window.window_elapsed(now, HTTP_RATE_WINDOW));
    }
    limited
}

fn rate_limited_response() -> Response {
    (
        StatusCode::TOO_MANY_REQUESTS,
        [(RETRY_AFTER, "60")],
        Json(json!({
            "error": {
                "code": "RATE_LIMITED",
                "message": "操作过于频繁，请稍后重试",
                "details": null
            }
        })),
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    use axum::body::Body;
    use axum::extract::ConnectInfo;
    use std::net::SocketAddr;
    use tower::ServiceExt;

    use super::*;

    fn request(uri: &str) -> Request {
        Request::builder().uri(uri).body(Body::empty()).unwrap()
    }

    fn engine_request(uri: &str) -> Request {
        let mut request = request(uri);
        request
            .extensions_mut()
            .insert(ConnectInfo(SocketAddr::from(([127, 0, 0, 1], 30_004))));
        request
    }

    async fn engine_status(app: &Router, uri: &str) -> StatusCode {
        let response = app.clone().oneshot(engine_request(uri)).await.unwrap();
        response.status()
    }

    #[test]
    fn engine_gate_matches_socketioxide_prefix_and_only_initial_opens() {
        for (uri, expected) in [
            ("/socket.io/?EIO=4&transport=websocket", true),
            ("/socket.io/anything?EIO=4&transport=polling", true),
            ("/socket.ioevil?EIO=4&transport=websocket", true),
            (
                "/socket.io/?EIO=4&transport=polling&sid=AAAAAAAAAAAAAAHs",
                false,
            ),
            ("/socket.io/?EIO=4&transport=websocket&sid=!", true),
            ("/socket.io/?EIO=4&transport=websocket&sid=", true),
            ("/api/rooms", false),
        ] {
            let actual = is_initial_engine_handshake(&request(uri));
            assert_eq!(actual, expected, "{uri}");
        }
    }

    #[tokio::test]
    async fn engine_gate_limits_opens_but_not_established_polling() {
        let app = Router::new()
            .fallback(|| async { StatusCode::NO_CONTENT })
            .layer(middleware::from_fn_with_state(
                HttpRateLimiter {
                    initial_engine_handshakes_only: true,
                    ..HttpRateLimiter::default()
                },
                enforce_rate_limit,
            ));

        for _ in 0..HTTP_RATE_MAX {
            let status = engine_status(&app, "/socket.io/?EIO=4&transport=polling").await;
            assert_eq!(status, StatusCode::NO_CONTENT);
        }
        let status = engine_status(
            &app,
            "/socket.io/?EIO=4&transport=polling&sid=AAAAAAAAAAAAAAHs",
        )
        .await;
        assert_eq!(status, StatusCode::NO_CONTENT);

        let status = engine_status(&app, "/socket.io/?EIO=4&transport=websocket").await;
        assert_eq!(status, StatusCode::TOO_MANY_REQUESTS);
    }
}
