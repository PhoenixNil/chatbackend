use std::collections::BTreeMap;

use axum::http::Uri;
use chrono::{DateTime, Duration, Utc};
use hmac::{Hmac, Mac};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::config::Config;
use crate::errors::AppError;

type HmacSha256 = Hmac<Sha256>;

#[derive(Clone)]
pub struct AvatarService {
    endpoint: String,
    public_base_url: String,
    bucket: String,
    region: String,
    access_key_id: String,
    secret_access_key: String,
    avatar_prefix: String,
    chat_image_prefix: String,
    max_bytes: u64,
    url_ttl_secs: i64,
}

#[derive(Debug, Clone)]
pub struct PresignedUpload {
    pub upload_url: String,
    pub method: String,
    pub headers: BTreeMap<String, String>,
    pub object_key: String,
    pub public_url: String,
    pub expires_at: DateTime<Utc>,
}

impl AvatarService {
    pub fn new(config: Config) -> Self {
        Self {
            endpoint: config
                .object_storage_endpoint
                .trim_end_matches('/')
                .to_string(),
            public_base_url: config
                .object_storage_public_base_url
                .trim_end_matches('/')
                .to_string(),
            bucket: config.object_storage_bucket.trim_matches('/').to_string(),
            region: config.object_storage_region,
            access_key_id: config.object_storage_access_key_id,
            secret_access_key: config.object_storage_secret_access_key,
            avatar_prefix: config
                .object_storage_avatar_prefix
                .trim_matches('/')
                .to_string(),
            chat_image_prefix: config
                .object_storage_chat_image_prefix
                .trim_matches('/')
                .to_string(),
            max_bytes: config.avatar_upload_max_bytes,
            url_ttl_secs: config.avatar_upload_url_ttl_secs,
        }
    }

    pub fn create_avatar_upload(
        &self,
        user_id: Uuid,
        file_name: &str,
        content_type: &str,
        size_bytes: u64,
    ) -> Result<PresignedUpload, AppError> {
        let object_key = self.object_key_for_user(user_id, content_type);
        self.create_image_upload(
            object_key,
            file_name,
            content_type,
            size_bytes,
            self.max_bytes,
        )
    }

    pub fn create_chat_image_upload(
        &self,
        user_id: Uuid,
        chat_id: Uuid,
        file_name: &str,
        content_type: &str,
        size_bytes: u64,
    ) -> Result<PresignedUpload, AppError> {
        let object_key = self.object_key_for_chat_image(chat_id, user_id, content_type);
        self.create_image_upload(
            object_key,
            file_name,
            content_type,
            size_bytes,
            self.max_bytes,
        )
    }

    pub fn validate_image_upload(
        &self,
        file_name: &str,
        content_type: &str,
        size_bytes: u64,
    ) -> Result<(), AppError> {
        self.validate_image_payload(file_name, content_type, size_bytes, self.max_bytes)
    }

    pub fn public_url_for_chat_image_object(
        &self,
        user_id: Uuid,
        chat_id: Uuid,
        object_key: &str,
    ) -> Result<String, AppError> {
        let trimmed_key = object_key.trim().trim_matches('/');
        if trimmed_key.is_empty() {
            return Err(AppError::Validation(
                "image object key cannot be empty".to_string(),
            ));
        }

        let expected_prefix = self.chat_image_object_prefix(chat_id, user_id);
        let expected_prefix_with_separator = format!("{expected_prefix}/");
        if !trimmed_key.starts_with(&expected_prefix_with_separator) {
            return Err(AppError::Validation(
                "image object key does not belong to current user and chat".to_string(),
            ));
        }

        Ok(self.public_url_for_key(trimmed_key))
    }

    fn create_image_upload(
        &self,
        object_key: String,
        file_name: &str,
        content_type: &str,
        size_bytes: u64,
        max_bytes: u64,
    ) -> Result<PresignedUpload, AppError> {
        self.validate_image_payload(file_name, content_type, size_bytes, max_bytes)?;

        let now = Utc::now();
        let expires_at = now
            .checked_add_signed(Duration::seconds(self.url_ttl_secs))
            .ok_or_else(|| {
                AppError::Internal("failed to compute avatar upload expiration".to_string())
            })?;
        let timestamp = now.format("%Y%m%dT%H%M%SZ").to_string();
        let datestamp = now.format("%Y%m%d").to_string();
        let public_url = self.public_url_for_key(&object_key);

        let endpoint_uri: Uri = self.endpoint.parse().map_err(|error| {
            AppError::Internal(format!("invalid object storage endpoint: {error}"))
        })?;
        let scheme = endpoint_uri.scheme_str().ok_or_else(|| {
            AppError::Internal("object storage endpoint is missing scheme".to_string())
        })?;
        let host = endpoint_uri
            .authority()
            .map(|authority| authority.as_str().to_string())
            .ok_or_else(|| {
                AppError::Internal("object storage endpoint is missing host".to_string())
            })?;
        let endpoint_origin = format!("{scheme}://{host}");
        let base_path = endpoint_uri.path().trim_end_matches('/');
        let canonical_uri = format!(
            "{}/{}",
            join_url_path(base_path, &self.bucket),
            aws_percent_encode(&object_key, false)
        );
        let credential_scope = format!("{datestamp}/{}/s3/aws4_request", self.region);
        let signed_headers = "host";
        let credential = format!("{}/{}", self.access_key_id, credential_scope);

        let mut query_pairs = BTreeMap::new();
        query_pairs.insert(
            "X-Amz-Algorithm".to_string(),
            "AWS4-HMAC-SHA256".to_string(),
        );
        query_pairs.insert(
            "X-Amz-Content-Sha256".to_string(),
            "UNSIGNED-PAYLOAD".to_string(),
        );
        query_pairs.insert("X-Amz-Credential".to_string(), credential);
        query_pairs.insert("X-Amz-Date".to_string(), timestamp.clone());
        query_pairs.insert("X-Amz-Expires".to_string(), self.url_ttl_secs.to_string());
        query_pairs.insert(
            "X-Amz-SignedHeaders".to_string(),
            signed_headers.to_string(),
        );

        let canonical_query = query_pairs
            .iter()
            .map(|(key, value)| {
                format!(
                    "{}={}",
                    aws_percent_encode(key, true),
                    aws_percent_encode(value, true)
                )
            })
            .collect::<Vec<_>>()
            .join("&");
        let canonical_headers = format!("host:{host}\n");
        let canonical_request = format!(
            "PUT\n{}\n{}\n{}\n{}\nUNSIGNED-PAYLOAD",
            canonical_uri, canonical_query, canonical_headers, signed_headers
        );
        let canonical_request_hash = hex::encode(Sha256::digest(canonical_request.as_bytes()));
        let string_to_sign = format!(
            "AWS4-HMAC-SHA256\n{}\n{}\n{}",
            timestamp, credential_scope, canonical_request_hash
        );
        let signing_key = derive_signing_key(&self.secret_access_key, &datestamp, &self.region)?;
        let signature = sign_hex(&signing_key, &string_to_sign)?;
        let upload_url = format!(
            "{}{}?{}&X-Amz-Signature={}",
            endpoint_origin, canonical_uri, canonical_query, signature
        );

        let headers = BTreeMap::new();

        Ok(PresignedUpload {
            upload_url,
            method: "PUT".to_string(),
            headers,
            object_key,
            public_url,
            expires_at,
        })
    }

    pub fn public_url_for_user_object(
        &self,
        user_id: Uuid,
        object_key: &str,
    ) -> Result<String, AppError> {
        let trimmed_key = object_key.trim().trim_matches('/');
        if trimmed_key.is_empty() {
            return Err(AppError::Validation(
                "avatar object key cannot be empty".to_string(),
            ));
        }

        let expected_prefix = self.user_prefix(user_id);
        let expected_prefix_with_separator = format!("{expected_prefix}/");
        if !trimmed_key.starts_with(&expected_prefix_with_separator) {
            return Err(AppError::Validation(
                "avatar object key does not belong to current user".to_string(),
            ));
        }

        Ok(self.public_url_for_key(trimmed_key))
    }

    fn validate_image_payload(
        &self,
        file_name: &str,
        content_type: &str,
        size_bytes: u64,
        max_bytes: u64,
    ) -> Result<(), AppError> {
        if file_name.trim().is_empty() {
            return Err(AppError::Validation(
                "file name cannot be empty".to_string(),
            ));
        }

        if size_bytes == 0 {
            return Err(AppError::Validation(
                "image file cannot be empty".to_string(),
            ));
        }

        if size_bytes > max_bytes {
            return Err(AppError::Validation(format!(
                "image file size must be at most {} bytes",
                max_bytes
            )));
        }

        if !allowed_content_type(content_type.trim()) {
            return Err(AppError::Validation(
                "image content type must be one of image/jpeg, image/png, image/webp, image/gif, or image/avif"
                    .to_string(),
            ));
        }

        Ok(())
    }

    fn object_key_for_user(&self, user_id: Uuid, content_type: &str) -> String {
        let extension = extension_for_content_type(content_type.trim()).unwrap_or("bin");
        format!(
            "{}/{}-{}.{}",
            self.user_prefix(user_id),
            Utc::now().timestamp_millis(),
            Uuid::new_v4().simple(),
            extension
        )
    }

    fn object_key_for_chat_image(
        &self,
        chat_id: Uuid,
        user_id: Uuid,
        content_type: &str,
    ) -> String {
        let extension = extension_for_content_type(content_type.trim()).unwrap_or("bin");
        format!(
            "{}/{}-{}.{}",
            self.chat_image_object_prefix(chat_id, user_id),
            Utc::now().timestamp_millis(),
            Uuid::new_v4().simple(),
            extension
        )
    }

    fn public_url_for_key(&self, object_key: &str) -> String {
        format!(
            "{}/{}",
            self.public_base_url,
            aws_percent_encode(object_key, false)
        )
    }

    fn user_prefix(&self, user_id: Uuid) -> String {
        if self.avatar_prefix.is_empty() {
            user_id.to_string()
        } else {
            format!("{}/{}", self.avatar_prefix, user_id)
        }
    }

    fn chat_image_object_prefix(&self, chat_id: Uuid, user_id: Uuid) -> String {
        if self.chat_image_prefix.is_empty() {
            format!("{chat_id}/{user_id}")
        } else {
            format!("{}/{chat_id}/{user_id}", self.chat_image_prefix)
        }
    }
}

fn allowed_content_type(value: &str) -> bool {
    matches!(
        value,
        "image/jpeg" | "image/png" | "image/webp" | "image/gif" | "image/avif"
    )
}

fn extension_for_content_type(value: &str) -> Option<&'static str> {
    match value {
        "image/jpeg" => Some("jpg"),
        "image/png" => Some("png"),
        "image/webp" => Some("webp"),
        "image/gif" => Some("gif"),
        "image/avif" => Some("avif"),
        _ => None,
    }
}

fn join_url_path(base: &str, value: &str) -> String {
    match (base.trim_matches('/'), value.trim_matches('/')) {
        ("", "") => "/".to_string(),
        ("", value) => format!("/{value}"),
        (base, "") => format!("/{base}"),
        (base, value) => format!("/{base}/{value}"),
    }
}

fn aws_percent_encode(value: &str, encode_slash: bool) -> String {
    let mut output = String::with_capacity(value.len());

    for byte in value.as_bytes() {
        let is_unreserved = matches!(
            byte,
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~'
        );
        if is_unreserved || (!encode_slash && *byte == b'/') {
            output.push(*byte as char);
        } else {
            output.push('%');
            output.push_str(&format!("{byte:02X}"));
        }
    }

    output
}

fn derive_signing_key(secret: &str, datestamp: &str, region: &str) -> Result<Vec<u8>, AppError> {
    let k_date = sign_bytes(format!("AWS4{secret}").as_bytes(), datestamp)?;
    let k_region = sign_bytes(&k_date, region)?;
    let k_service = sign_bytes(&k_region, "s3")?;
    sign_bytes(&k_service, "aws4_request")
}

fn sign_bytes(key: &[u8], message: &str) -> Result<Vec<u8>, AppError> {
    let mut mac = HmacSha256::new_from_slice(key).map_err(|error| {
        AppError::Internal(format!("failed to initialize HMAC signer: {error}"))
    })?;
    mac.update(message.as_bytes());
    Ok(mac.finalize().into_bytes().to_vec())
}

fn sign_hex(key: &[u8], message: &str) -> Result<String, AppError> {
    Ok(hex::encode(sign_bytes(key, message)?))
}

#[cfg(test)]
mod tests {
    use uuid::Uuid;

    use super::AvatarService;
    use crate::config::Config;

    fn test_config() -> Config {
        Config {
            host: "127.0.0.1".to_string(),
            port: 8080,
            database_url: "postgres://postgres:postgres@localhost:5432/chatbackend".to_string(),
            db_max_connections: 80,
            db_min_connections: 10,
            db_connect_timeout_secs: 10,
            db_acquire_timeout_secs: 10,
            db_idle_timeout_secs: 600,
            db_max_lifetime_secs: 1800,
            jwt_secret: "secret".to_string(),
            jwt_expire_minutes: 60,
            object_storage_endpoint: "http://127.0.0.1:9000".to_string(),
            object_storage_public_base_url: "http://127.0.0.1:9000/chatbackend".to_string(),
            object_storage_bucket: "chatbackend".to_string(),
            object_storage_region: "auto".to_string(),
            object_storage_access_key_id: "minioadmin".to_string(),
            object_storage_secret_access_key: "minioadmin".to_string(),
            object_storage_avatar_prefix: "avatars".to_string(),
            object_storage_chat_image_prefix: "chat-images".to_string(),
            avatar_upload_max_bytes: 5 * 1024 * 1024,
            avatar_upload_url_ttl_secs: 300,
        }
    }

    #[test]
    fn avatar_upload_keys_use_avatar_prefix() {
        let service = AvatarService::new(test_config());
        let user_id = Uuid::parse_str("23a0f37e-2534-4af2-91b7-c988fb049400").unwrap();

        let upload = service
            .create_avatar_upload(user_id, "avatar.png", "image/png", 1024)
            .unwrap();

        assert!(
            upload
                .object_key
                .starts_with(&format!("avatars/{user_id}/"))
        );
        assert!(upload.object_key.ends_with(".png"));
    }

    #[test]
    fn chat_image_upload_keys_use_chat_image_prefix() {
        let service = AvatarService::new(test_config());
        let user_id = Uuid::parse_str("23a0f37e-2534-4af2-91b7-c988fb049400").unwrap();
        let chat_id = Uuid::parse_str("7dc72832-52d7-44e2-ac5a-bf83f6f7818f").unwrap();

        let upload = service
            .create_chat_image_upload(user_id, chat_id, "image.webp", "image/webp", 2048)
            .unwrap();

        assert!(
            upload
                .object_key
                .starts_with(&format!("chat-images/{chat_id}/{user_id}/"))
        );
        assert!(upload.object_key.ends_with(".webp"));
    }

    #[test]
    fn user_object_urls_only_accept_avatar_prefix() {
        let service = AvatarService::new(test_config());
        let user_id = Uuid::parse_str("23a0f37e-2534-4af2-91b7-c988fb049400").unwrap();
        let valid_key = format!("avatars/{user_id}/avatar.png");
        let invalid_key = format!("chat-images/room/{user_id}/avatar.png");

        let url = service
            .public_url_for_user_object(user_id, &valid_key)
            .unwrap();

        assert_eq!(
            url,
            format!("http://127.0.0.1:9000/chatbackend/{valid_key}")
        );
        assert!(
            service
                .public_url_for_user_object(user_id, &invalid_key)
                .is_err()
        );
    }

    #[test]
    fn chat_image_urls_require_matching_chat_and_user_prefix() {
        let service = AvatarService::new(test_config());
        let user_id = Uuid::parse_str("23a0f37e-2534-4af2-91b7-c988fb049400").unwrap();
        let chat_id = Uuid::parse_str("7dc72832-52d7-44e2-ac5a-bf83f6f7818f").unwrap();
        let other_chat_id = Uuid::parse_str("7efbd560-645d-42c0-aebf-ff84f00d7f54").unwrap();
        let valid_key = format!("chat-images/{chat_id}/{user_id}/image.png");
        let wrong_chat_key = format!("chat-images/{other_chat_id}/{user_id}/image.png");
        let wrong_prefix_key = format!("avatars/{user_id}/image.png");

        let url = service
            .public_url_for_chat_image_object(user_id, chat_id, &valid_key)
            .unwrap();

        assert_eq!(
            url,
            format!("http://127.0.0.1:9000/chatbackend/{valid_key}")
        );
        assert!(
            service
                .public_url_for_chat_image_object(user_id, chat_id, &wrong_chat_key)
                .is_err()
        );
        assert!(
            service
                .public_url_for_chat_image_object(user_id, chat_id, &wrong_prefix_key)
                .is_err()
        );
    }
}
