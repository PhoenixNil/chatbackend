pub mod auth;
pub mod chats;
pub mod users;
pub mod ws;

use axum::Json;
use axum::Router;
use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use axum::http::{HeaderMap, Method, header};
use axum::routing::{get, post, put};
use serde::Serialize;
use tower_http::cors::{Any, CorsLayer};
use tower_http::trace::TraceLayer;
use uuid::Uuid;

use crate::errors::AppError;
use crate::state::{AppState, SharedAppState};

//API
pub fn router(state: SharedAppState) -> Router {
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods([
            Method::GET,
            Method::POST,
            Method::PUT,
            Method::PATCH,
            Method::OPTIONS,
        ])
        .allow_headers(Any);

    Router::new()
        .route("/health", get(health))
        .route("/ready", get(health))
        .route("/api/auth/register", post(auth::register))
        .route("/api/auth/login", post(auth::login))
        .route("/api/users/me", get(users::me))
        .route(
            "/api/users/me/avatar/upload-url",
            post(users::avatar_upload_url),
        )
        .route(
            "/api/users/me/avatar",
            put(users::update_avatar).patch(users::update_avatar),
        )
        .route(
            "/api/chats",
            post(chats::create_chat).get(chats::list_chats),
        )
        .route(
            "/api/chats/{chat_id}/members",
            get(chats::members).post(chats::add_members),
        )
        .route(
            "/api/chats/{chat_id}/messages/search",
            get(chats::search_messages),
        )
        .route(
            "/api/chats/{chat_id}/images/upload-url",
            post(chats::image_upload_url),
        )
        .route(
            "/api/chats/{chat_id}/messages",
            get(chats::history).post(chats::create_image_message),
        )
        .route("/api/chats/{chat_id}/read", post(chats::mark_read_up_to))
        .route("/api/chats/{chat_id}/leave", post(chats::leave_chat))
        .route("/ws", get(ws::upgrade))
        .with_state(state)
        .layer(TraceLayer::new_for_http())
        .layer(cors)
}

#[derive(Debug, Serialize)]
struct HealthResponse {
    status: &'static str,
    service: &'static str,
}

async fn health() -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok",
        service: "chatbackend",
    })
}

#[derive(Debug, Clone, Copy)]
pub struct AuthUser(pub Uuid);

impl FromRequestParts<SharedAppState> for AuthUser {
    type Rejection = AppError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &SharedAppState,
    ) -> Result<Self, Self::Rejection> {
        Ok(Self(user_id_from_headers(state.as_ref(), &parts.headers)?))
    }
}

pub fn user_id_from_token(state: &AppState, token: &str) -> Result<Uuid, AppError> {
    state.auth_service.validate_token(token)
}

fn user_id_from_headers(state: &AppState, headers: &HeaderMap) -> Result<Uuid, AppError> {
    let token = bearer_token(headers)?;
    user_id_from_token(state, token)
}

fn bearer_token(headers: &HeaderMap) -> Result<&str, AppError> {
    let raw = headers
        .get(header::AUTHORIZATION)
        .ok_or(AppError::Unauthorized)?
        .to_str()
        .map_err(|_| AppError::Unauthorized)?;

    let token = raw
        .strip_prefix("Bearer ")
        .or_else(|| raw.strip_prefix("bearer "))
        .ok_or(AppError::Unauthorized)?
        .trim();

    if token.is_empty() {
        return Err(AppError::Unauthorized);
    }

    Ok(token)
}
