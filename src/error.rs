use axum::{
    http::{header::RETRY_AFTER, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use serde_json::json;
use std::time::Duration;
use thiserror::Error;

pub type AppResult<T> = Result<T, AppError>;

#[derive(Debug, Error)]
pub enum AppError {
    #[error("resource not found")]
    NotFound,
    #[error("{0}")]
    Validation(String),
    #[error("{0}")]
    Unauthorized(String),
    #[error("{service} rate limit reached during {operation}; retry after {retry_after_secs}s")]
    RateLimited {
        service: &'static str,
        operation: String,
        retry_after_secs: u64,
    },
    #[error(transparent)]
    Internal(#[from] anyhow::Error),
}

impl From<sqlx::Error> for AppError {
    fn from(error: sqlx::Error) -> Self {
        match error {
            sqlx::Error::RowNotFound => Self::NotFound,
            sqlx::Error::Database(db_error) => match db_error.code().as_deref() {
                Some("23503") => Self::Validation("referenced record does not exist".to_string()),
                Some("23505") => Self::Validation("record already exists".to_string()),
                Some("23514") => Self::Validation("database validation failed".to_string()),
                _ => Self::Internal(anyhow::Error::new(sqlx::Error::Database(db_error))),
            },
            other => Self::Internal(anyhow::Error::new(other)),
        }
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let status = match &self {
            Self::NotFound => StatusCode::NOT_FOUND,
            Self::Validation(_) => StatusCode::BAD_REQUEST,
            Self::Unauthorized(_) => StatusCode::UNAUTHORIZED,
            Self::RateLimited { .. } => StatusCode::TOO_MANY_REQUESTS,
            Self::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
        };

        let client_error = match &self {
            Self::NotFound => self.to_string(),
            Self::Validation(_) => self.to_string(),
            Self::Unauthorized(_) => self.to_string(),
            Self::RateLimited {
                retry_after_secs, ..
            } => format!("too many requests; retry after {retry_after_secs}s"),
            Self::Internal(error) => {
                tracing::error!(error = ?error, "internal server error");
                "internal server error".to_string()
            }
        };

        let body = Json(json!({
            "error": client_error,
        }));

        let mut response = (status, body).into_response();
        if let Some(retry_after) = self.retry_after() {
            let seconds = retry_after.as_secs().max(1).to_string();
            if let Ok(value) = HeaderValue::from_str(&seconds) {
                response.headers_mut().insert(RETRY_AFTER, value);
            }
        }

        response
    }
}

impl AppError {
    pub fn retry_after(&self) -> Option<Duration> {
        match self {
            Self::RateLimited {
                retry_after_secs, ..
            } => Some(Duration::from_secs(*retry_after_secs)),
            _ => None,
        }
    }
}
