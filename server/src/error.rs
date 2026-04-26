use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde_json::json;
use shared::Article;

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("not found")]
    NotFound,
    #[error("{0}")]
    BadRequest(String),
    #[error("conflict")]
    Conflict(Box<Article>),
    #[error("文献重复")]
    DuplicateArticle,
    #[error(transparent)]
    Internal(#[from] anyhow::Error),
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, msg) = match &self {
            AppError::NotFound => (StatusCode::NOT_FOUND, "not found".to_string()),
            AppError::BadRequest(m) => (StatusCode::BAD_REQUEST, m.clone()),
            AppError::Conflict(article) => {
                let body = axum::Json(json!({
                    "error": "conflict",
                    "message": "article was updated by another client",
                    "article": article,
                }));
                return (StatusCode::CONFLICT, body).into_response();
            }
            AppError::DuplicateArticle => {
                let body = axum::Json(json!({ "error": "文献重复" }));
                return (StatusCode::CONFLICT, body).into_response();
            }
            AppError::Internal(e) => {
                tracing::error!("internal error: {e:?}");
                (StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
            }
        };
        let body = axum::Json(json!({ "error": msg }));
        (status, body).into_response()
    }
}

pub type AppResult<T> = Result<T, AppError>;
