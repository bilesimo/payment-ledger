use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde::Serialize;
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    AccountNotFound,
    IdempotencyConflict,
    Infrastructure,
    InvalidConfiguration,
    InvalidMoneyAmount,
    InvalidRequest,
    ReversalNotAllowed,
    TransactionNotFound,
    UnbalancedTransaction,
}

#[derive(Debug, Error, Clone)]
pub enum AppError {
    #[error("{message}")]
    Validation { code: ErrorCode, message: String },
    #[error("{message}")]
    NotFound { code: ErrorCode, message: String },
    #[error("{message}")]
    Conflict { code: ErrorCode, message: String },
    #[error("{message}")]
    Unexpected { code: ErrorCode, message: String },
}

impl AppError {
    pub fn validation(code: ErrorCode, message: String) -> Self {
        Self::Validation { code, message }
    }

    pub fn not_found(code: ErrorCode, message: String) -> Self {
        Self::NotFound { code, message }
    }

    pub fn conflict(code: ErrorCode, message: String) -> Self {
        Self::Conflict { code, message }
    }

    pub fn unexpected(code: ErrorCode, message: String) -> Self {
        Self::Unexpected { code, message }
    }

    pub fn status_code(&self) -> StatusCode {
        match self {
            Self::Validation { .. } => StatusCode::UNPROCESSABLE_ENTITY,
            Self::NotFound { .. } => StatusCode::NOT_FOUND,
            Self::Conflict { .. } => StatusCode::CONFLICT,
            Self::Unexpected { .. } => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    pub fn code(&self) -> ErrorCode {
        match self {
            Self::Validation { code, .. }
            | Self::NotFound { code, .. }
            | Self::Conflict { code, .. }
            | Self::Unexpected { code, .. } => *code,
        }
    }

    pub fn message(&self) -> &str {
        match self {
            Self::Validation { message, .. }
            | Self::NotFound { message, .. }
            | Self::Conflict { message, .. }
            | Self::Unexpected { message, .. } => message,
        }
    }
}

#[derive(Debug, Serialize)]
struct ErrorResponse {
    code: ErrorCode,
    message: String,
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let body = ErrorResponse {
            code: self.code(),
            message: self.message().to_owned(),
        };

        (self.status_code(), Json(body)).into_response()
    }
}
