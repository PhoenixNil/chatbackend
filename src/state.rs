use std::time::Duration;

use sea_orm::{ConnectOptions, Database, DatabaseConnection};

use crate::config::Config;
use crate::errors::AppError;
use crate::repository::chat_repo::ChatRepository;
use crate::repository::message_repo::MessageRepository;
use crate::repository::user_repo::UserRepository;
use crate::rooms::manager::RoomManager;
use crate::service::auth_service::AuthService;
use crate::service::avatar_service::AvatarService;
use crate::service::chat_service::ChatService;

#[derive(Clone)]
pub struct AppState {
    #[allow(dead_code)]
    pub db: DatabaseConnection,
    pub rooms: RoomManager,
    pub auth_service: AuthService,
    pub chat_service: ChatService,
    pub avatar_service: AvatarService,
}

impl AppState {
    pub async fn new(config: &Config) -> Result<Self, AppError> {
        let mut connect_options = ConnectOptions::new(config.database_url.clone());
        connect_options
            .max_connections(config.db_max_connections)
            .min_connections(config.db_min_connections)
            .connect_timeout(Duration::from_secs(config.db_connect_timeout_secs))
            .acquire_timeout(Duration::from_secs(config.db_acquire_timeout_secs))
            .idle_timeout(Duration::from_secs(config.db_idle_timeout_secs))
            .max_lifetime(Duration::from_secs(config.db_max_lifetime_secs));

        let db = Database::connect(connect_options).await?;
        let rooms = RoomManager::default();

        let user_repo = UserRepository::new(db.clone());
        let chat_repo = ChatRepository::new(db.clone());
        let message_repo = MessageRepository::new(db.clone());

        let auth_service = AuthService::new(
            user_repo.clone(),
            config.jwt_secret.clone(),
            config.jwt_expire_minutes,
        );
        let chat_service = ChatService::new(chat_repo, message_repo, user_repo);
        let avatar_service = AvatarService::new(config.clone());

        Ok(Self {
            db,
            rooms,
            auth_service,
            chat_service,
            avatar_service,
        })
    }
}
