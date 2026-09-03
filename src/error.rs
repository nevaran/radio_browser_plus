// Error definitions and HTTP response handling
use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde_json::json;

use thiserror::Error;

/// Application error type
#[derive(Debug, Error)]
pub enum AppError {
    #[error("Unauthorized: {0}")]
    Unauthorized(String),

    #[error("Forbidden: {0}")]
    Forbidden(String),

    #[error("Not found: {0}")]
    NotFound(String),

    #[error("Bad request: {0}")]
    BadRequest(String),

    #[error("Internal server error: {0}")]
    InternalError(String),

    #[error("External service error: {0}")]
    ExternalServiceError(String),

    #[error("Storage error: {0}")]
    StorageError(String),

    #[error("Serialization error: {0}")]
    SerializationError(#[from] serde_json::Error),

    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("HTTP client error: {0}")]
    ReqwestError(#[from] reqwest::Error),
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, message) = match self {
            Self::Unauthorized(msg) => (StatusCode::UNAUTHORIZED, msg),
            Self::Forbidden(msg) => (StatusCode::FORBIDDEN, msg),
            Self::NotFound(msg) => (StatusCode::NOT_FOUND, msg),
            Self::BadRequest(msg) => (StatusCode::BAD_REQUEST, msg),
            Self::InternalError(msg) => (StatusCode::INTERNAL_SERVER_ERROR, msg),
            Self::ExternalServiceError(msg) => (StatusCode::BAD_GATEWAY, msg),
            Self::StorageError(msg) => (StatusCode::INTERNAL_SERVER_ERROR, msg),
            Self::SerializationError(err) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Serialization error: {}", err),
            ),
            Self::IoError(err) => (StatusCode::INTERNAL_SERVER_ERROR, format!("IO error: {}", err)),
            Self::ReqwestError(err) => (
                StatusCode::BAD_GATEWAY,
                format!("External API error: {}", err),
            ),
        };

        tracing::error!(error = %message, "API error");

        (status, Json(json!({"error": message}))).into_response()
    }
}

pub type Result<T> = std::result::Result<T, AppError>;
