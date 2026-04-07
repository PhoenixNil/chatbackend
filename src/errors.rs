use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::Serialize;
use thiserror::Error;
use tracing::error;
use uuid::Uuid;
use validator::ValidationErrors;

// Centralized application error type used by HTTP handlers and websocket flows.
#[derive(Debug, Error)]
pub enum AppError {
    #[error("unauthorized")]
    Unauthorized,
    #[error("forbidden")]
    Forbidden,
    #[error("not found: {0}")]
    NotFound(String),
    #[error("validation failed: {0}")]
    Validation(String),
    #[error("conflict: {0}")]
    Conflict(String),
    #[error("internal error")]
    Internal(String),
}

impl AppError {
    pub fn status_code(&self) -> StatusCode {
        match self {
            Self::Unauthorized => StatusCode::UNAUTHORIZED,
            Self::Forbidden => StatusCode::FORBIDDEN,
            Self::NotFound(_) => StatusCode::NOT_FOUND,
            Self::Validation(_) => StatusCode::UNPROCESSABLE_ENTITY,
            Self::Conflict(_) => StatusCode::CONFLICT,
            Self::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    pub fn code(&self) -> &'static str {
        match self {
            Self::Unauthorized => "UNAUTHORIZED",
            Self::Forbidden => "FORBIDDEN",
            Self::NotFound(_) => "NOT_FOUND",
            Self::Validation(_) => "VALIDATION_ERROR",
            Self::Conflict(_) => "CONFLICT",
            Self::Internal(_) => "INTERNAL_ERROR",
        }
    }

    pub fn client_message(&self) -> String {
        match self {
            // Never expose internal implementation details to clients.
            Self::Internal(_) => "internal server error".to_string(),
            _ => self.to_string(),
        }
    }
}

#[derive(Debug, Serialize)]
struct ErrorBody {
    code: String,
    message: String,
    // Correlation id returned to clients and included in server logs.
    request_id: String,
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        // Keep the original internal message only for server-side logging.
        let internal_message = match &self {
            Self::Internal(message) => Some(message.clone()),
            _ => None,
        };
        let status = self.status_code();
        let code = self.code().to_string();
        let client_message = self.client_message();
        let request_id = Uuid::new_v4().to_string();

        if let Some(internal) = internal_message {
            error!(
                request_id = %request_id,
                status = %status,
                code = %code,
                internal_error = %internal,
                "request failed with internal error"
            );
        }

        let body = ErrorBody {
            code,
            message: client_message,
            request_id,
        };

        (status, Json(body)).into_response()
    }
}

impl From<sea_orm::DbErr> for AppError {
    fn from(value: sea_orm::DbErr) -> Self {
        Self::Internal(value.to_string())
    }
}

impl From<serde_json::Error> for AppError {
    fn from(value: serde_json::Error) -> Self {
        Self::Validation(value.to_string())
    }
}

impl From<ValidationErrors> for AppError {
    fn from(value: ValidationErrors) -> Self {
        // Flatten field-level validation failures into a stable, deduplicated
        // client-facing message to keep API responses predictable.
        let mut details = value
            .field_errors()
            .iter()
            .flat_map(|(field, errors)| {
                errors
                    .iter()
                    .map(move |error| format!("{field}: {}", error.code.as_ref()))
            })
            .collect::<Vec<_>>();
        details.sort();
        details.dedup();

        if details.is_empty() {
            return Self::Validation("invalid request".to_string());
        }

        Self::Validation(details.join(", "))
    }
}
