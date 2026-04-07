use std::collections::HashMap;

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::HeaderMap;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use validator::Validate;

use crate::errors::AppError;
use crate::models::chat::Chat;
use crate::models::message::{Message, MessageSearchResult};
use crate::service::avatar_service::PresignedUpload;
use crate::service::chat_service::CreateImageMessageInput;
use crate::state::AppState;
use crate::websocket::protocol::{ServerMsg, UnreadReason};

#[derive(Debug, Deserialize, Validate)]
pub struct CreateChatRequest {
    #[validate(length(min = 1, max = 64))]
    #[validate(custom(function = "crate::validation::validate_trimmed_not_empty"))]
    pub name: String,
    #[validate(custom(function = "crate::validation::validate_chat_type"))]
    pub chat_type: Option<String>,
    #[validate(length(min = 1))]
    pub members: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct HistoryQuery {
    pub before: Option<DateTime<Utc>>,
    pub limit: Option<u64>,
}

#[derive(Debug, Deserialize)]
pub struct SearchMessagesQuery {
    pub q: Option<String>,
    pub before: Option<DateTime<Utc>>,
    pub limit: Option<u64>,
}

#[derive(Debug, Deserialize, Validate)]
pub struct MarkReadUpToRequest {
    pub up_to_seq: i64,
}

#[derive(Debug, Serialize)]
pub struct MarkReadUpToResponse {
    pub chat_id: Uuid,
    pub user_id: Uuid,
    pub up_to_seq: i64,
    pub delta: i64,
}

#[derive(Debug, Serialize)]
pub struct ChatMemberResponse {
    pub id: Uuid,
    pub username: String,
    pub avatar_url: Option<String>,
}

#[derive(Debug, Deserialize, Validate)]
pub struct AddMembersRequest {
    #[validate(length(min = 1))]
    pub members: Vec<String>,
}

#[derive(Debug, Deserialize, Validate)]
pub struct CreateImageUploadRequest {
    #[validate(length(min = 1, max = 255))]
    pub file_name: String,
    #[validate(length(min = 1, max = 64))]
    pub content_type: String,
    pub size_bytes: u64,
}

#[derive(Debug, Serialize)]
pub struct CreateImageUploadResponse {
    pub upload_url: String,
    pub method: String,
    pub headers: std::collections::BTreeMap<String, String>,
    pub object_key: String,
    pub public_url: String,
    pub expires_at: String,
}

impl From<PresignedUpload> for CreateImageUploadResponse {
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

#[derive(Debug, Deserialize, Validate)]
pub struct CreateImageMessageRequest {
    #[validate(length(min = 1, max = 1024))]
    pub object_key: String,
    #[validate(length(max = 2000))]
    pub caption: Option<String>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub size_bytes: u64,
    #[validate(length(min = 1, max = 64))]
    pub content_type: String,
}

pub async fn create_chat(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<CreateChatRequest>,
) -> Result<Json<Chat>, AppError> {
    payload.validate()?;

    let user_id = super::user_id_from_headers(&state, &headers)?;
    let chat_type = payload.chat_type.unwrap_or_else(|| "group".to_string());

    let chat = state
        .chat_service
        .create_chat(user_id, &payload.name, &chat_type, payload.members)
        .await?;

    Ok(Json(chat))
}

pub async fn list_chats(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Vec<Chat>>, AppError> {
    let user_id = super::user_id_from_headers(&state, &headers)?;
    let chats = state.chat_service.list_chats(user_id).await?;
    Ok(Json(chats))
}

pub async fn image_upload_url(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(chat_id): Path<Uuid>,
    Json(payload): Json<CreateImageUploadRequest>,
) -> Result<Json<CreateImageUploadResponse>, AppError> {
    payload.validate()?;

    let user_id = super::user_id_from_headers(&state, &headers)?;
    state
        .chat_service
        .require_chat_member_ids_for_user(chat_id, user_id)
        .await?;
    state.avatar_service.validate_image_upload(
        &payload.file_name,
        &payload.content_type,
        payload.size_bytes,
    )?;

    let upload = state.avatar_service.create_chat_image_upload(
        user_id,
        chat_id,
        &payload.file_name,
        &payload.content_type,
        payload.size_bytes,
    )?;

    Ok(Json(upload.into()))
}

pub async fn history(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(chat_id): Path<Uuid>,
    Query(query): Query<HistoryQuery>,
) -> Result<Json<Vec<Message>>, AppError> {
    let user_id = super::user_id_from_headers(&state, &headers)?;
    let limit = query.limit.unwrap_or(50).clamp(1, 100);

    let messages = state
        .chat_service
        .list_messages(user_id, chat_id, query.before, limit)
        .await?;

    Ok(Json(messages))
}

pub async fn create_image_message(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(chat_id): Path<Uuid>,
    Json(payload): Json<CreateImageMessageRequest>,
) -> Result<Json<Message>, AppError> {
    payload.validate()?;

    let user_id = super::user_id_from_headers(&state, &headers)?;
    state
        .chat_service
        .require_chat_member_ids_for_user(chat_id, user_id)
        .await?;
    state.avatar_service.validate_image_upload(
        payload.object_key.as_str(),
        &payload.content_type,
        payload.size_bytes,
    )?;

    let media_url = state.avatar_service.public_url_for_chat_image_object(
        user_id,
        chat_id,
        payload.object_key.trim(),
    )?;
    let message = state
        .chat_service
        .send_image_message(
            user_id,
            chat_id,
            CreateImageMessageInput {
                body: payload.caption.unwrap_or_default(),
                media_url,
                media_width: normalize_media_dimension(payload.width, "width")?,
                media_height: normalize_media_dimension(payload.height, "height")?,
                media_size_bytes: i64::try_from(payload.size_bytes).map_err(|_| {
                    AppError::Validation("image file size is too large".to_string())
                })?,
                media_content_type: payload.content_type.trim().to_string(),
            },
        )
        .await?;

    broadcast_new_message(&state, user_id, &message);

    Ok(Json(message.message))
}

pub async fn search_messages(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(chat_id): Path<Uuid>,
    Query(query): Query<SearchMessagesQuery>,
) -> Result<Json<Vec<MessageSearchResult>>, AppError> {
    let user_id = super::user_id_from_headers(&state, &headers)?;
    let limit = query.limit.unwrap_or(20).clamp(1, 50);
    let search_query = query.q.unwrap_or_default();

    let messages = state
        .chat_service
        .search_messages(user_id, chat_id, &search_query, query.before, limit)
        .await?;

    Ok(Json(messages))
}

pub async fn mark_read_up_to(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(chat_id): Path<Uuid>,
    Json(payload): Json<MarkReadUpToRequest>,
) -> Result<Json<MarkReadUpToResponse>, AppError> {
    payload.validate()?;

    let user_id = super::user_id_from_headers(&state, &headers)?;
    let result = state
        .chat_service
        .mark_read_up_to(user_id, chat_id, payload.up_to_seq)
        .await?;

    Ok(Json(MarkReadUpToResponse {
        chat_id: result.chat_id,
        user_id: result.user_id,
        up_to_seq: result.up_to_seq,
        delta: result.delta,
    }))
}

pub async fn add_members(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(chat_id): Path<Uuid>,
    Json(payload): Json<AddMembersRequest>,
) -> Result<Json<Vec<ChatMemberResponse>>, AppError> {
    payload.validate()?;

    let user_id = super::user_id_from_headers(&state, &headers)?;
    let members = state
        .chat_service
        .add_members(user_id, chat_id, payload.members)
        .await?;

    Ok(Json(
        members
            .into_iter()
            .map(|member| ChatMemberResponse {
                id: member.id,
                username: member.username,
                avatar_url: member.avatar_url,
            })
            .collect(),
    ))
}

fn normalize_media_dimension(
    value: Option<u32>,
    field_name: &str,
) -> Result<Option<i32>, AppError> {
    let Some(value) = value else {
        return Ok(None);
    };

    if value == 0 {
        return Err(AppError::Validation(format!(
            "image {field_name} must be greater than 0"
        )));
    }

    i32::try_from(value)
        .map(Some)
        .map_err(|_| AppError::Validation(format!("image {field_name} is out of range")))
}

fn broadcast_new_message(
    state: &AppState,
    sender_id: Uuid,
    result: &crate::service::chat_service::SendMessageResult,
) {
    state.rooms.broadcast_to_users(
        &result.member_ids,
        ServerMsg::NewMessage {
            chat_id: result.message.chat_id,
            message_id: result.message.id,
            seq: result.message.seq,
            sender_id: result.message.sender_id,
            body: result.message.body.clone(),
            message_type: result.message.message_type.clone(),
            media_url: result.message.media_url.clone(),
            media_width: result.message.media_width,
            media_height: result.message.media_height,
            media_size_bytes: result.message.media_size_bytes,
            media_content_type: result.message.media_content_type.clone(),
            created_at: result.message.created_at,
        },
    );

    let recipients = result
        .member_ids
        .iter()
        .copied()
        .filter(|member_id| *member_id != sender_id)
        .collect::<Vec<_>>();
    if recipients.is_empty() {
        return;
    }

    state.rooms.send_unread_delta_to_users(
        &recipients,
        result.message.chat_id,
        1,
        UnreadReason::NewMessage,
        &HashMap::new(),
    );
}

pub async fn members(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(chat_id): Path<Uuid>,
) -> Result<Json<Vec<ChatMemberResponse>>, AppError> {
    let user_id = super::user_id_from_headers(&state, &headers)?;
    let members = state
        .chat_service
        .list_chat_members(user_id, chat_id)
        .await?;

    Ok(Json(
        members
            .into_iter()
            .map(|member| ChatMemberResponse {
                id: member.id,
                username: member.username,
                avatar_url: member.avatar_url,
            })
            .collect(),
    ))
}
