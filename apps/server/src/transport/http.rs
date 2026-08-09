use axum::{
    Json, Router,
    extract::{Path, State, rejection::JsonRejection},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::{
    domain::errors::RuleError, rooms::room_service::RoomService, transport::utf16_len,
    views::room_view::create_room_view,
};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct DisplayNameBody {
    display_name: String,
}

#[derive(Debug, Serialize)]
struct ErrorEnvelope {
    error: ErrorPayload,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ErrorPayload {
    pub code: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<Value>,
}

#[derive(Debug)]
enum HttpError {
    InvalidRequest,
    Rule(RuleError),
}

impl From<RuleError> for HttpError {
    fn from(error: RuleError) -> Self {
        Self::Rule(error)
    }
}

impl IntoResponse for HttpError {
    fn into_response(self) -> Response {
        let (status, error) = match self {
            Self::InvalidRequest => (
                StatusCode::BAD_REQUEST,
                ErrorPayload {
                    code: "INVALID_REQUEST".to_owned(),
                    message: "请求格式无效".to_owned(),
                    details: None,
                },
            ),
            Self::Rule(error) => {
                let status = status_for_rule_error(&error);
                (status, rule_error_payload(error))
            }
        };
        (status, Json(ErrorEnvelope { error })).into_response()
    }
}

fn status_for_rule_error(error: &RuleError) -> StatusCode {
    match error.code.as_str() {
        "ROOM_NOT_FOUND" => StatusCode::NOT_FOUND,
        "INVALID_CREDENTIALS" => StatusCode::UNAUTHORIZED,
        "DISPLAY_NAME_TAKEN" | "ROOM_FULL" | "STALE_STATE" => StatusCode::CONFLICT,
        "SERVER_BUSY" => StatusCode::SERVICE_UNAVAILABLE,
        "INTERNAL_ERROR" => StatusCode::INTERNAL_SERVER_ERROR,
        _ => StatusCode::BAD_REQUEST,
    }
}

pub fn routes(rooms: RoomService) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/api/rooms", post(create_room))
        .route("/api/rooms/{room_code}", get(get_public_room))
        .route("/api/rooms/{room_code}/join", post(join_room))
        .with_state(rooms)
}

async fn health() -> Json<Value> {
    Json(json!({ "ok": true }))
}

async fn create_room(
    State(rooms): State<RoomService>,
    body: Result<Json<DisplayNameBody>, JsonRejection>,
) -> Result<impl IntoResponse, HttpError> {
    let Json(body) = body.map_err(|_| HttpError::InvalidRequest)?;
    let display_name = validated_display_name(body.display_name)?;
    let credentials = rooms.create_room(&display_name)?;
    Ok((StatusCode::CREATED, Json(credentials)))
}

async fn join_room(
    State(rooms): State<RoomService>,
    Path(room_code): Path<String>,
    body: Result<Json<DisplayNameBody>, JsonRejection>,
) -> Result<Json<impl Serialize>, HttpError> {
    let room_code = validated_room_code(room_code)?;
    let Json(body) = body.map_err(|_| HttpError::InvalidRequest)?;
    let display_name = validated_display_name(body.display_name)?;
    Ok(Json(rooms.join_room(&room_code, &display_name)?))
}

async fn get_public_room(
    State(rooms): State<RoomService>,
    Path(room_code): Path<String>,
) -> Result<Json<Value>, HttpError> {
    let room_code = validated_room_code(room_code)?;
    let room = rooms.require_room(&room_code)?;
    Ok(Json(create_room_view(&room, None)))
}

fn validated_display_name(value: String) -> Result<String, HttpError> {
    let trimmed = value.trim();
    if !(1..=20).contains(&utf16_len(trimmed)) {
        return Err(HttpError::InvalidRequest);
    }
    Ok(trimmed.to_owned())
}

pub(crate) fn validated_socket_room_code(value: String) -> Result<String, ErrorPayload> {
    let trimmed = value.trim();
    if !(4..=12).contains(&utf16_len(trimmed)) {
        return Err(invalid_message("roomCode must contain 4 to 12 characters"));
    }
    Ok(trimmed.to_uppercase())
}

fn validated_room_code(value: String) -> Result<String, HttpError> {
    validated_socket_room_code(value).map_err(|_| HttpError::InvalidRequest)
}

pub(crate) fn invalid_message(message: impl Into<String>) -> ErrorPayload {
    ErrorPayload {
        code: "INVALID_MESSAGE".to_owned(),
        message: "消息格式无效".to_owned(),
        details: Some(json!([{ "message": message.into() }])),
    }
}

pub(crate) fn rule_error_payload(error: RuleError) -> ErrorPayload {
    if error.code == "INTERNAL_ERROR" {
        return internal_error_payload();
    }
    ErrorPayload {
        code: error.code,
        message: error.message,
        details: Some(error.details.unwrap_or(Value::Null)),
    }
}

pub(crate) fn internal_error_payload() -> ErrorPayload {
    ErrorPayload {
        code: "INTERNAL_ERROR".to_owned(),
        message: "服务器内部错误".to_owned(),
        details: None,
    }
}
