use std::collections::{HashMap, HashSet};

use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::entities::{chats, users};
use crate::errors::AppError;
use crate::models::chat::Chat;
use crate::models::message::{IMAGE_MESSAGE_TYPE, Message, MessageSearchResult, TEXT_MESSAGE_TYPE};
use crate::repository::chat_repo::ChatRepository;
use crate::repository::message_repo::{MessageRepository, NewMessageRecord};
use crate::repository::user_repo::UserRepository;
use crate::validation::{ensure_trimmed_not_empty, ensure_username};

#[derive(Debug, Clone)]
pub struct SendMessageResult {
    pub message: Message,
    pub member_ids: Vec<Uuid>,
}

#[derive(Debug, Clone)]
pub struct MarkReadResult {
    pub message_id: Uuid,
    pub chat_id: Uuid,
    pub user_id: Uuid,
    pub read_at: DateTime<Utc>,
    /// Negative unread delta for the reader, or 0 if the cursor did not advance.
    pub delta: i64,
}

#[derive(Debug, Clone)]
pub struct MarkReadUpToResult {
    pub chat_id: Uuid,
    pub user_id: Uuid,
    pub up_to_seq: i64,
    /// Negative unread delta for the reader, or 0 if the cursor did not advance.
    pub delta: i64,
}

#[derive(Debug, Clone)]
pub struct ChatMemberResult {
    pub id: Uuid,
    pub username: String,
    pub avatar_url: Option<String>,
}

#[derive(Debug, Clone)]
pub struct CreateImageMessageInput {
    pub body: String,
    pub media_url: String,
    pub media_width: Option<i32>,
    pub media_height: Option<i32>,
    pub media_size_bytes: i64,
    pub media_content_type: String,
}

#[derive(Clone)]
pub struct ChatService {
    chat_repo: ChatRepository,
    message_repo: MessageRepository,
    user_repo: UserRepository,
}

impl ChatService {
    pub fn new(
        chat_repo: ChatRepository,
        message_repo: MessageRepository,
        user_repo: UserRepository,
    ) -> Self {
        Self {
            chat_repo,
            message_repo,
            user_repo,
        }
    }

    pub async fn create_chat(
        &self,
        creator_id: Uuid,
        name: &str,
        chat_type: &str,
        members: Vec<String>,
    ) -> Result<Chat, AppError> {
        let trimmed_name = ensure_trimmed_not_empty(name, "chat name cannot be empty")?;
        let member_ids = self
            .resolve_chat_member_ids(creator_id, members, chat_type)
            .await?;
        let chat = if chat_type == "direct" {
            let other_user_id = member_ids
                .iter()
                .copied()
                .find(|user_id| *user_id != creator_id)
                .ok_or_else(|| {
                    AppError::Validation("direct chat must include another user".to_string())
                })?;

            self.chat_repo
                .find_or_create_direct_chat(trimmed_name.to_string(), creator_id, other_user_id)
                .await?
        } else {
            self.chat_repo
                .create_chat_with_members(
                    trimmed_name.to_string(),
                    chat_type.to_string(),
                    member_ids,
                )
                .await?
        };

        self.load_chat_for_user(chat.id, creator_id).await
    }

    pub async fn list_chats(&self, user_id: Uuid) -> Result<Vec<Chat>, AppError> {
        let chat_rows = self.chat_repo.list_user_chats_with_stats(user_id).await?;
        Ok(chat_rows
            .into_iter()
            .map(|row| Chat::from_model(row.chat, row.member_count, row.unread_count))
            .collect())
    }

    pub async fn list_messages(
        &self,
        user_id: Uuid,
        chat_id: Uuid,
        before: Option<DateTime<Utc>>,
        limit: u64,
    ) -> Result<Vec<Message>, AppError> {
        self.ensure_chat_member(chat_id, user_id).await?;
        let messages = self
            .message_repo
            .list_messages(user_id, chat_id, before, limit)
            .await?
            .unwrap_or_default();

        Ok(messages.into_iter().map(Into::into).collect())
    }

    pub async fn search_messages(
        &self,
        user_id: Uuid,
        chat_id: Uuid,
        query: &str,
        before: Option<DateTime<Utc>>,
        limit: u64,
    ) -> Result<Vec<MessageSearchResult>, AppError> {
        self.ensure_chat_member(chat_id, user_id).await?;

        let trimmed_query = query.trim();
        if trimmed_query.is_empty() {
            return Ok(Vec::new());
        }

        self.message_repo
            .search_messages(chat_id, trimmed_query, before, limit)
            .await
    }

    pub async fn send_message(
        &self,
        sender_id: Uuid,
        chat_id: Uuid,
        body: String,
    ) -> Result<SendMessageResult, AppError> {
        let trimmed = ensure_trimmed_not_empty(&body, "message body cannot be empty")?;
        let member_ids = self
            .require_chat_member_ids_for_user(chat_id, sender_id)
            .await?;

        let message = self
            .message_repo
            .insert_message_and_touch_chat(NewMessageRecord {
                chat_id,
                sender_id,
                body: trimmed.to_string(),
                message_type: TEXT_MESSAGE_TYPE.to_string(),
                media_url: None,
                media_width: None,
                media_height: None,
                media_size_bytes: None,
                media_content_type: None,
            })
            .await?;

        Ok(SendMessageResult {
            message: message.into(),
            member_ids,
        })
    }

    pub async fn send_image_message(
        &self,
        sender_id: Uuid,
        chat_id: Uuid,
        payload: CreateImageMessageInput,
    ) -> Result<SendMessageResult, AppError> {
        let member_ids = self
            .require_chat_member_ids_for_user(chat_id, sender_id)
            .await?;

        let message = self
            .message_repo
            .insert_message_and_touch_chat(NewMessageRecord {
                chat_id,
                sender_id,
                body: payload.body.trim().to_string(),
                message_type: IMAGE_MESSAGE_TYPE.to_string(),
                media_url: Some(payload.media_url),
                media_width: payload.media_width,
                media_height: payload.media_height,
                media_size_bytes: Some(payload.media_size_bytes),
                media_content_type: Some(payload.media_content_type),
            })
            .await?;

        Ok(SendMessageResult {
            message: message.into(),
            member_ids,
        })
    }

    /// Legacy: mark read by message_id (looks up seq server-side).
    pub async fn mark_read(
        &self,
        user_id: Uuid,
        message_id: Uuid,
    ) -> Result<MarkReadResult, AppError> {
        let (chat_id, seq) = self
            .message_repo
            .find_seq_by_id(message_id)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("message {message_id}")))?;

        let read = self.mark_read_up_to(user_id, chat_id, seq).await?;

        Ok(MarkReadResult {
            message_id,
            chat_id: read.chat_id,
            user_id: read.user_id,
            read_at: Utc::now(),
            delta: read.delta,
        })
    }

    /// Cursor-based: advance the read pointer for user in chat.
    pub async fn mark_read_up_to(
        &self,
        user_id: Uuid,
        chat_id: Uuid,
        seq: i64,
    ) -> Result<MarkReadUpToResult, AppError> {
        self.ensure_chat_member(chat_id, user_id).await?;

        let (_old_seq, new_seq, advanced_count) = self
            .chat_repo
            .mark_read_up_to(chat_id, user_id, seq)
            .await?;

        let delta = if advanced_count > 0 {
            -advanced_count
        } else {
            0
        };

        Ok(MarkReadUpToResult {
            chat_id,
            user_id,
            up_to_seq: new_seq,
            delta,
        })
    }

    pub async fn add_members(
        &self,
        requester_id: Uuid,
        chat_id: Uuid,
        usernames: Vec<String>,
    ) -> Result<Vec<ChatMemberResult>, AppError> {
        let chat = self.require_chat(chat_id).await?;

        if chat.chat_type == "direct" {
            return Err(AppError::Validation(
                "cannot add members to a direct chat".to_string(),
            ));
        }

        self.ensure_chat_member(chat_id, requester_id).await?;

        let mut existing_member_ids = self.chat_member_id_set(chat_id).await?;
        let added_users = self.resolve_users_by_usernames(usernames, false).await?;
        let new_users = added_users
            .into_iter()
            .filter(|user| existing_member_ids.insert(user.id))
            .collect::<Vec<_>>();
        let new_ids = new_users.iter().map(|user| user.id).collect::<Vec<_>>();

        if !new_ids.is_empty() {
            self.chat_repo.insert_members(chat_id, &new_ids).await?;
        }

        Ok(new_users
            .into_iter()
            .map(Self::chat_member_result_from_user)
            .collect())
    }

    pub async fn list_chat_members(
        &self,
        requester_id: Uuid,
        chat_id: Uuid,
    ) -> Result<Vec<ChatMemberResult>, AppError> {
        self.require_chat(chat_id).await?;
        self.ensure_chat_member(chat_id, requester_id).await?;

        self.load_chat_member_results(chat_id).await
    }

    async fn load_chat_for_user(&self, chat_id: Uuid, user_id: Uuid) -> Result<Chat, AppError> {
        let chat_with_stats = self
            .chat_repo
            .find_chat_with_stats_for_user(chat_id, user_id)
            .await?
            .ok_or_else(|| {
                AppError::Internal(format!(
                    "failed to load chat {} after create or lookup",
                    chat_id
                ))
            })?;

        Ok(Chat::from_model(
            chat_with_stats.chat,
            chat_with_stats.member_count,
            chat_with_stats.unread_count,
        ))
    }

    async fn require_chat(&self, chat_id: Uuid) -> Result<chats::Model, AppError> {
        self.chat_repo
            .find_by_id(chat_id)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("chat {chat_id}")))
    }

    pub async fn require_chat_member_ids_for_user(
        &self,
        chat_id: Uuid,
        user_id: Uuid,
    ) -> Result<Vec<Uuid>, AppError> {
        let member_ids = self.require_chat_member_ids(chat_id).await?;
        self.ensure_member_in_list(&member_ids, user_id)?;
        Ok(member_ids)
    }

    async fn ensure_chat_member(&self, chat_id: Uuid, user_id: Uuid) -> Result<(), AppError> {
        if self.chat_repo.is_member(chat_id, user_id).await? {
            return Ok(());
        }

        Err(AppError::Forbidden)
    }

    async fn require_chat_member_ids(&self, chat_id: Uuid) -> Result<Vec<Uuid>, AppError> {
        let member_ids = self.chat_repo.list_member_ids(chat_id).await?;
        if member_ids.is_empty() {
            return Err(AppError::NotFound(format!("chat {chat_id}")));
        }

        Ok(member_ids)
    }

    async fn chat_member_id_set(&self, chat_id: Uuid) -> Result<HashSet<Uuid>, AppError> {
        Ok(self
            .require_chat_member_ids(chat_id)
            .await?
            .into_iter()
            .collect())
    }

    fn ensure_member_in_list(&self, member_ids: &[Uuid], user_id: Uuid) -> Result<(), AppError> {
        if member_ids.contains(&user_id) {
            return Ok(());
        }

        Err(AppError::Forbidden)
    }

    async fn resolve_chat_member_ids(
        &self,
        creator_id: Uuid,
        usernames: Vec<String>,
        chat_type: &str,
    ) -> Result<Vec<Uuid>, AppError> {
        let mut member_ids = HashSet::from([creator_id]);
        let users = self.resolve_users_by_usernames(usernames, true).await?;
        for user in users {
            member_ids.insert(user.id);
        }

        if chat_type == "direct" && member_ids.len() != 2 {
            return Err(AppError::Validation(
                "direct chat must include exactly one other user".to_string(),
            ));
        }

        if member_ids.len() < 2 {
            return Err(AppError::Validation(
                "chat must include at least 2 users".to_string(),
            ));
        }

        Ok(member_ids.into_iter().collect())
    }

    async fn resolve_users_by_usernames(
        &self,
        usernames: Vec<String>,
        reject_blank: bool,
    ) -> Result<Vec<users::Model>, AppError> {
        let mut users = Vec::new();
        for username in Self::normalize_member_usernames(usernames, reject_blank)? {
            users.push(self.find_user_by_username(&username).await?);
        }

        Ok(users)
    }

    fn normalize_member_usernames(
        usernames: Vec<String>,
        reject_blank: bool,
    ) -> Result<Vec<String>, AppError> {
        let mut normalized_usernames = Vec::new();
        let mut seen_usernames = HashSet::new();

        for username in usernames {
            let trimmed = username.trim();
            if trimmed.is_empty() {
                if reject_blank {
                    return Err(AppError::Validation(
                        "member username cannot be empty".to_string(),
                    ));
                }
                continue;
            }

            let normalized = ensure_username(trimmed, "member username")?.to_string();
            if seen_usernames.insert(normalized.clone()) {
                normalized_usernames.push(normalized);
            }
        }

        Ok(normalized_usernames)
    }

    async fn find_user_by_username(&self, username: &str) -> Result<users::Model, AppError> {
        self.user_repo
            .find_by_username(username)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("user {username}")))
    }

    async fn load_chat_member_results(
        &self,
        chat_id: Uuid,
    ) -> Result<Vec<ChatMemberResult>, AppError> {
        let member_ids = self.require_chat_member_ids(chat_id).await?;
        let users = self.user_repo.list_by_ids(member_ids.clone()).await?;
        let users_by_id = users
            .into_iter()
            .map(|user| (user.id, user))
            .collect::<HashMap<_, _>>();

        let members = member_ids
            .into_iter()
            .filter_map(|id| {
                users_by_id
                    .get(&id)
                    .cloned()
                    .map(Self::chat_member_result_from_user)
            })
            .collect();

        Ok(members)
    }

    fn chat_member_result_from_user(user: users::Model) -> ChatMemberResult {
        ChatMemberResult {
            id: user.id,
            username: user.username,
            avatar_url: user.avatar_url,
        }
    }
}
