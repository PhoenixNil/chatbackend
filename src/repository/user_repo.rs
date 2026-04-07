use chrono::Utc;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, Set, SqlErr,
};
use uuid::Uuid;

use crate::entities::users;
use crate::errors::AppError;

#[derive(Clone)]
pub struct UserRepository {
    db: DatabaseConnection,
}

impl UserRepository {
    pub fn new(db: DatabaseConnection) -> Self {
        Self { db }
    }

    pub async fn create_user(
        &self,
        username: String,
        password_hash: String,
    ) -> Result<users::Model, AppError> {
        let user = users::ActiveModel {
            id: Set(Uuid::new_v4()),
            username: Set(username),
            password_hash: Set(password_hash),
            avatar_url: Set(None),
            created_at: Set(Utc::now().fixed_offset()),
        }
        .insert(&self.db)
        .await
        .map_err(|error| match error.sql_err() {
            Some(SqlErr::UniqueConstraintViolation(_)) => {
                AppError::Conflict("username already exists".to_string())
            }
            _ => AppError::from(error),
        })?;

        Ok(user)
    }

    pub async fn find_by_id(&self, user_id: Uuid) -> Result<Option<users::Model>, AppError> {
        let user = users::Entity::find_by_id(user_id).one(&self.db).await?;
        Ok(user)
    }

    pub async fn find_by_username(&self, username: &str) -> Result<Option<users::Model>, AppError> {
        let user = users::Entity::find()
            .filter(users::Column::Username.eq(username))
            .one(&self.db)
            .await?;
        Ok(user)
    }

    pub async fn list_by_ids(&self, user_ids: Vec<Uuid>) -> Result<Vec<users::Model>, AppError> {
        if user_ids.is_empty() {
            return Ok(Vec::new());
        }

        let users = users::Entity::find()
            .filter(users::Column::Id.is_in(user_ids))
            .all(&self.db)
            .await?;

        Ok(users)
    }

    pub async fn update_avatar_url(
        &self,
        user_id: Uuid,
        avatar_url: Option<String>,
    ) -> Result<users::Model, AppError> {
        let user = self
            .find_by_id(user_id)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("user {user_id}")))?;

        let mut active_model: users::ActiveModel = user.into();
        active_model.avatar_url = Set(avatar_url);
        let updated = active_model.update(&self.db).await?;

        Ok(updated)
    }
}
