use crate::entities::users::Model as UserEntity;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct User {
    pub id: Uuid,
    pub username: String,
    pub avatar_url: Option<String>,
    pub created_at: DateTime<Utc>,
}

impl From<UserEntity> for User {
    fn from(value: UserEntity) -> Self {
        Self {
            id: value.id,
            username: value.username,
            avatar_url: value.avatar_url,
            created_at: value.created_at.with_timezone(&Utc),
        }
    }
}
