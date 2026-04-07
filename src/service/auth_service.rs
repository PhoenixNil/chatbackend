use argon2::Argon2;
use argon2::password_hash::rand_core::OsRng;
use argon2::password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use chrono::{Duration, Utc};
use jsonwebtoken::{DecodingKey, EncodingKey, Header, Validation, decode, encode};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::errors::AppError;
use crate::models::user::User;
use crate::repository::user_repo::UserRepository;
use crate::validation::{ensure_password, ensure_username};

#[derive(Debug, Serialize, Deserialize)]
struct Claims {
    sub: String,
    exp: usize,
}

#[derive(Clone)]
pub struct AuthService {
    user_repo: UserRepository,
    jwt_secret: String,
    jwt_expire_minutes: i64,
}

impl AuthService {
    pub fn new(user_repo: UserRepository, jwt_secret: String, jwt_expire_minutes: i64) -> Self {
        Self {
            user_repo,
            jwt_secret,
            jwt_expire_minutes,
        }
    }

    pub async fn register(&self, username: &str, password: &str) -> Result<String, AppError> {
        let normalized_username = ensure_username(username, "username")?;
        ensure_password(password)?;

        if self
            .user_repo
            .find_by_username(normalized_username)
            .await?
            .is_some()
        {
            return Err(AppError::Conflict("username already exists".to_string()));
        }

        let password_hash = Self::hash_password(password)?;
        let user = self
            .user_repo
            .create_user(normalized_username.to_string(), password_hash)
            .await?;

        self.issue_token(user.id)
    }

    pub async fn login(&self, username: &str, password: &str) -> Result<String, AppError> {
        let normalized_username = username.trim();
        let user = self
            .user_repo
            .find_by_username(normalized_username)
            .await?
            .ok_or(AppError::Unauthorized)?;

        Self::verify_password(password, &user.password_hash)?;
        self.issue_token(user.id)
    }

    pub fn validate_token(&self, token: &str) -> Result<Uuid, AppError> {
        //验证token
        let claims = decode::<Claims>(
            token,
            &DecodingKey::from_secret(self.jwt_secret.as_bytes()),
            &Validation::default(),
        )
        .map_err(|_| AppError::Unauthorized)?;

        Uuid::parse_str(&claims.claims.sub).map_err(|_| AppError::Unauthorized) //取出sub并解析token中的用户id
    }

    pub async fn user_profile(&self, user_id: Uuid) -> Result<User, AppError> {
        let user = self
            .user_repo
            .find_by_id(user_id)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("user {user_id}")))?;

        Ok(user.into())
    }

    pub async fn update_user_avatar(
        &self,
        user_id: Uuid,
        avatar_url: Option<String>,
    ) -> Result<User, AppError> {
        let user = self
            .user_repo
            .update_avatar_url(user_id, avatar_url)
            .await?;
        Ok(user.into())
    }

    fn issue_token(&self, user_id: Uuid) -> Result<String, AppError> {
        //登录注册后签发token
        let exp = Utc::now()
            .checked_add_signed(Duration::minutes(self.jwt_expire_minutes))
            .ok_or_else(|| AppError::Internal("failed to compute token expiration".to_string()))?
            .timestamp() as usize;

        let claims = Claims {
            sub: user_id.to_string(),
            exp,
        };

        let token = encode(
            &Header::default(),
            &claims,
            &EncodingKey::from_secret(self.jwt_secret.as_bytes()),
        )
        .map_err(|error| AppError::Internal(error.to_string()))?;

        Ok(token)
    }

    fn hash_password(password: &str) -> Result<String, AppError> {
        let salt = SaltString::generate(&mut OsRng);
        let hash = Argon2::default()
            .hash_password(password.as_bytes(), &salt)
            .map_err(|error| AppError::Internal(error.to_string()))?
            .to_string();

        Ok(hash)
    }

    fn verify_password(password: &str, password_hash: &str) -> Result<(), AppError> {
        let parsed = PasswordHash::new(password_hash).map_err(|_| AppError::Unauthorized)?;
        Argon2::default()
            .verify_password(password.as_bytes(), &parsed)
            .map_err(|_| AppError::Unauthorized)
    }
}
