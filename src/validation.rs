use crate::errors::AppError;
use validator::ValidationError;

pub fn validate_trimmed_not_empty(value: &str) -> Result<(), ValidationError> {
    if value.trim().is_empty() {
        return Err(ValidationError::new("blank"));
    }

    Ok(())
}

pub fn validate_username_chars(value: &str) -> Result<(), ValidationError> {
    if value.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
        return Ok(());
    }

    Err(ValidationError::new("username_chars"))
}

pub fn validate_chat_type(chat_type: &str) -> Result<(), ValidationError> {
    match chat_type {
        "group" | "direct" => Ok(()),
        _ => Err(ValidationError::new("chat_type")),
    }
}

pub fn ensure_trimmed_not_empty<'a>(value: &'a str, message: &str) -> Result<&'a str, AppError> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(AppError::Validation(message.to_string()));
    }

    Ok(trimmed)
}

pub fn ensure_username<'a>(value: &'a str, field_name: &str) -> Result<&'a str, AppError> {
    let trimmed = ensure_trimmed_not_empty(value, &format!("{field_name} cannot be empty"))?;
    if trimmed.len() < 3 || trimmed.len() > 32 {
        return Err(AppError::Validation(format!(
            "{field_name} length must be between 3 and 32"
        )));
    }

    if validate_username_chars(trimmed).is_err() {
        return Err(AppError::Validation(format!(
            "{field_name} can only contain letters, numbers, and underscores"
        )));
    }

    Ok(trimmed)
}

pub fn ensure_password(password: &str) -> Result<(), AppError> {
    if password.len() < 8 {
        return Err(AppError::Validation(
            "password length must be at least 8".to_string(),
        ));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        ensure_password, ensure_trimmed_not_empty, ensure_username, validate_chat_type,
        validate_trimmed_not_empty, validate_username_chars,
    };

    #[test]
    fn username_accepts_alnum_and_underscore_only() {
        assert!(validate_username_chars("alice_01").is_ok());
        assert!(validate_username_chars("alice-01").is_err());
    }

    #[test]
    fn blank_validator_rejects_whitespace_only() {
        assert!(validate_trimmed_not_empty("hello").is_ok());
        assert!(validate_trimmed_not_empty("   ").is_err());
    }

    #[test]
    fn chat_type_validator_allows_only_group_or_direct() {
        assert!(validate_chat_type("group").is_ok());
        assert!(validate_chat_type("direct").is_ok());
        assert!(validate_chat_type("private").is_err());
    }

    #[test]
    fn ensure_username_reuses_service_level_validation_messages() {
        assert_eq!(
            ensure_username(" alice_01 ", "username").unwrap(),
            "alice_01"
        );
        assert!(ensure_username("a", "username").is_err());
        assert!(ensure_username("alice-01", "username").is_err());
    }

    #[test]
    fn ensure_trimmed_not_empty_preserves_trimmed_value() {
        assert_eq!(
            ensure_trimmed_not_empty(" hello ", "cannot be empty").unwrap(),
            "hello"
        );
    }

    #[test]
    fn ensure_password_requires_minimum_length() {
        assert!(ensure_password("12345678").is_ok());
        assert!(ensure_password("1234567").is_err());
    }
}
