use std::env;

const DEFAULT_AVATAR_PREFIX: &str = "avatars";
const DEFAULT_CHAT_IMAGE_PREFIX: &str = "chat-images";

#[derive(Debug, Clone)]
pub struct Config {
    pub host: String,
    pub port: u16,
    pub database_url: String,
    pub db_max_connections: u32,
    pub db_min_connections: u32,
    pub db_connect_timeout_secs: u64,
    pub db_acquire_timeout_secs: u64,
    pub db_idle_timeout_secs: u64,
    pub db_max_lifetime_secs: u64,
    pub jwt_secret: String,
    pub jwt_expire_minutes: i64,
    pub object_storage_endpoint: String,
    pub object_storage_public_base_url: String,
    pub object_storage_bucket: String,
    pub object_storage_region: String,
    pub object_storage_access_key_id: String,
    pub object_storage_secret_access_key: String,
    pub object_storage_avatar_prefix: String,
    pub object_storage_chat_image_prefix: String,
    pub avatar_upload_max_bytes: u64,
    pub avatar_upload_url_ttl_secs: i64,
}

impl Config {
    pub fn from_env() -> Self {
        let db_max_connections = env::var("DB_MAX_CONNECTIONS")
            .ok()
            .and_then(|raw| raw.parse::<u32>().ok())
            .filter(|value| *value > 0)
            .unwrap_or(80);
        let db_min_connections = env::var("DB_MIN_CONNECTIONS")
            .ok()
            .and_then(|raw| raw.parse::<u32>().ok())
            .filter(|value| *value > 0)
            .unwrap_or(10)
            .min(db_max_connections);

        Self {
            host: env::var("APP_HOST").unwrap_or_else(|_| "127.0.0.1".to_string()),
            port: env::var("APP_PORT")
                .ok()
                .and_then(|raw| raw.parse::<u16>().ok())
                .unwrap_or(8080),
            database_url: env::var("DATABASE_URL").unwrap_or_else(|_| {
                "postgres://postgres:postgres@localhost:5432/chatbackend".to_string()
            }),
            db_max_connections,
            db_min_connections,
            db_connect_timeout_secs: env::var("DB_CONNECT_TIMEOUT_SECONDS")
                .ok()
                .and_then(|raw| raw.parse::<u64>().ok())
                .filter(|value| *value > 0)
                .unwrap_or(10),
            db_acquire_timeout_secs: env::var("DB_ACQUIRE_TIMEOUT_SECONDS")
                .ok()
                .and_then(|raw| raw.parse::<u64>().ok())
                .filter(|value| *value > 0)
                .unwrap_or(10),
            db_idle_timeout_secs: env::var("DB_IDLE_TIMEOUT_SECONDS")
                .ok()
                .and_then(|raw| raw.parse::<u64>().ok())
                .filter(|value| *value > 0)
                .unwrap_or(600),
            db_max_lifetime_secs: env::var("DB_MAX_LIFETIME_SECONDS")
                .ok()
                .and_then(|raw| raw.parse::<u64>().ok())
                .filter(|value| *value > 0)
                .unwrap_or(1800),
            jwt_secret: env::var("JWT_SECRET").unwrap_or_else(|_| "change_me_in_prod".to_string()),
            jwt_expire_minutes: env::var("JWT_EXPIRE_MINUTES")
                .ok()
                .and_then(|raw| raw.parse::<i64>().ok())
                .unwrap_or(60 * 24),
            object_storage_endpoint: env::var("OBJECT_STORAGE_ENDPOINT")
                .unwrap_or_else(|_| "http://127.0.0.1:9000".to_string()),
            object_storage_public_base_url: env::var("OBJECT_STORAGE_PUBLIC_BASE_URL")
                .unwrap_or_else(|_| "http://127.0.0.1:9000/chatbackend".to_string()),
            object_storage_bucket: env::var("OBJECT_STORAGE_BUCKET")
                .unwrap_or_else(|_| "chatbackend".to_string()),
            object_storage_region: env::var("OBJECT_STORAGE_REGION")
                .unwrap_or_else(|_| "auto".to_string()),
            object_storage_access_key_id: env::var("OBJECT_STORAGE_ACCESS_KEY_ID")
                .unwrap_or_else(|_| "minioadmin".to_string()),
            object_storage_secret_access_key: env::var("OBJECT_STORAGE_SECRET_ACCESS_KEY")
                .unwrap_or_else(|_| "minioadmin".to_string()),
            object_storage_avatar_prefix: normalize_storage_prefix(
                env::var("OBJECT_STORAGE_AVATAR_PREFIX").ok().as_deref(),
                DEFAULT_AVATAR_PREFIX,
            ),
            object_storage_chat_image_prefix: normalize_storage_prefix(
                env::var("OBJECT_STORAGE_CHAT_IMAGE_PREFIX").ok().as_deref(),
                DEFAULT_CHAT_IMAGE_PREFIX,
            ),
            avatar_upload_max_bytes: env::var("AVATAR_UPLOAD_MAX_BYTES")
                .ok()
                .and_then(|raw| raw.parse::<u64>().ok())
                .filter(|value| *value > 0)
                .unwrap_or(5 * 1024 * 1024),
            avatar_upload_url_ttl_secs: env::var("AVATAR_UPLOAD_URL_TTL_SECONDS")
                .ok()
                .and_then(|raw| raw.parse::<i64>().ok())
                .filter(|value| *value > 0)
                .unwrap_or(300),
        }
    }
}

fn normalize_storage_prefix(value: Option<&str>, default: &str) -> String {
    value.unwrap_or(default).trim_matches('/').to_string()
}

#[cfg(test)]
mod tests {
    use super::{DEFAULT_AVATAR_PREFIX, DEFAULT_CHAT_IMAGE_PREFIX, normalize_storage_prefix};

    #[test]
    fn storage_prefix_defaults_when_missing() {
        assert_eq!(
            normalize_storage_prefix(None, DEFAULT_AVATAR_PREFIX),
            DEFAULT_AVATAR_PREFIX
        );
        assert_eq!(
            normalize_storage_prefix(None, DEFAULT_CHAT_IMAGE_PREFIX),
            DEFAULT_CHAT_IMAGE_PREFIX
        );
    }

    #[test]
    fn storage_prefix_trims_slashes() {
        assert_eq!(
            normalize_storage_prefix(Some("/avatars/custom/"), DEFAULT_AVATAR_PREFIX),
            "avatars/custom"
        );
        assert_eq!(
            normalize_storage_prefix(Some("/chat-images/custom/"), DEFAULT_CHAT_IMAGE_PREFIX),
            "chat-images/custom"
        );
        assert_eq!(
            normalize_storage_prefix(Some("///"), DEFAULT_CHAT_IMAGE_PREFIX),
            ""
        );
    }
}
