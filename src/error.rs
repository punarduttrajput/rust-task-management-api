use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde_json::json;

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("{0}")]
    BadRequest(String),
    #[error("{message}")]
    Unauthorized { code: &'static str, message: String },
    #[error("forbidden: {0}")]
    Forbidden(String),
    #[error("{0} not found")]
    NotFound(String),
    #[error("internal error")]
    Internal(String),
}

impl AppError {
    pub fn unauthorized(code: &'static str, message: impl Into<String>) -> Self {
        Self::Unauthorized { code, message: message.into() }
    }
}

impl From<sqlx::Error> for AppError {
    fn from(e: sqlx::Error) -> Self {
        Self::Internal(e.to_string())
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, code) = match &self {
            Self::BadRequest(_) => (StatusCode::BAD_REQUEST, "bad_request"),
            Self::Unauthorized { code, .. } => (StatusCode::UNAUTHORIZED, *code),
            Self::Forbidden(_) => (StatusCode::FORBIDDEN, "forbidden"),
            Self::NotFound(_) => (StatusCode::NOT_FOUND, "not_found"),
            Self::Internal(detail) => {
                tracing::error!(%detail, "internal error");
                (StatusCode::INTERNAL_SERVER_ERROR, "internal_error")
            }
        };
        let body = json!({ "error": { "code": code, "message": self.to_string() } });
        (status, Json(body)).into_response()
    }
}

pub type AppResult<T> = Result<T, AppError>;
