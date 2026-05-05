use axum::Json;
use axum::extract::State;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use validator::Validate;

use crate::errors::AppError;
use crate::models::user::User;
use crate::service::avatar_service::PresignedUpload;
use crate::state::SharedAppState;

#[derive(Debug, Deserialize, Validate)]
pub struct CreateAvatarUploadRequest {
    #[validate(length(min = 1, max = 255))]
    pub file_name: String,
    #[validate(length(min = 1, max = 64))]
    pub content_type: String,
    pub size_bytes: u64,
}

#[derive(Debug, Serialize)]
pub struct CreateAvatarUploadResponse {
    pub upload_url: String,
    pub method: String,
    pub headers: BTreeMap<String, String>,
    pub object_key: String,
    pub public_url: String,
    pub expires_at: String,
}

impl From<PresignedUpload> for CreateAvatarUploadResponse {
    fn from(value: PresignedUpload) -> Self {
        Self {
            upload_url: value.upload_url,
            method: value.method,
            headers: value.headers,
            object_key: value.object_key,
            public_url: value.public_url,
            expires_at: value.expires_at.to_rfc3339(),
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct UpdateAvatarRequest {
    pub object_key: Option<String>,
}

pub async fn me(
    State(state): State<SharedAppState>,
    super::AuthUser(user_id): super::AuthUser,
) -> Result<Json<User>, AppError> {
    let user = state.auth_service.user_profile(user_id).await?;
    Ok(Json(user))
}

pub async fn avatar_upload_url(
    State(state): State<SharedAppState>,
    super::AuthUser(user_id): super::AuthUser,
    Json(payload): Json<CreateAvatarUploadRequest>,
) -> Result<Json<CreateAvatarUploadResponse>, AppError> {
    payload.validate()?;
    state.avatar_service.validate_image_upload(
        &payload.file_name,
        &payload.content_type,
        payload.size_bytes,
    )?;
    let upload = state.avatar_service.create_avatar_upload(
        user_id,
        &payload.file_name,
        &payload.content_type,
        payload.size_bytes,
    )?;

    Ok(Json(upload.into()))
}

pub async fn update_avatar(
    State(state): State<SharedAppState>,
    super::AuthUser(user_id): super::AuthUser,
    Json(payload): Json<UpdateAvatarRequest>,
) -> Result<Json<User>, AppError> {
    let avatar_url = match payload.object_key {
        Some(object_key) => Some(
            state
                .avatar_service
                .public_url_for_user_object(user_id, object_key.trim())?,
        ),
        None => None,
    };

    let user = state
        .auth_service
        .update_user_avatar(user_id, avatar_url)
        .await?;

    Ok(Json(user))
}
