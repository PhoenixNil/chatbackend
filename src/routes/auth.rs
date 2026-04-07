use axum::Json;
use axum::extract::State;
use serde::{Deserialize, Serialize};
use validator::Validate;

use crate::errors::AppError;
use crate::state::AppState;

#[derive(Debug, Deserialize, Validate)]
pub struct RegisterRequest {
    #[validate(length(min = 3, max = 32))]
    #[validate(custom(function = "crate::validation::validate_username_chars"))]
    pub username: String,
    #[validate(length(min = 8))]
    pub password: String,
}

#[derive(Debug, Deserialize, Validate)]
pub struct LoginRequest {
    #[validate(length(min = 3, max = 32))]
    #[validate(custom(function = "crate::validation::validate_username_chars"))]
    pub username: String,
    #[validate(length(min = 8))]
    pub password: String,
}

#[derive(Debug, Serialize)]
pub struct AuthResponse {
    pub token: String,
}

pub async fn register(
    State(state): State<AppState>,
    Json(payload): Json<RegisterRequest>,
) -> Result<Json<AuthResponse>, AppError> {
    payload.validate()?;

    let token = state
        .auth_service
        .register(&payload.username, &payload.password)
        .await?;

    Ok(Json(AuthResponse { token }))
}

pub async fn login(
    State(state): State<AppState>,
    Json(payload): Json<LoginRequest>,
) -> Result<Json<AuthResponse>, AppError> {
    payload.validate()?;

    let token = state
        .auth_service
        .login(&payload.username, &payload.password)
        .await?;

    Ok(Json(AuthResponse { token }))
}
