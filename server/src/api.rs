use axum::extract::{Path, State};
use axum::routing::{delete, get, post};
use axum::{Json, Router};
use shared::{Article, ArticleUpdate, NewArticle, NewProject, Project};

use crate::error::{AppError, AppResult};
use crate::state::SharedState;
use crate::store::{AddResult, UpdateResult, DEFAULT_PROJECT_ID};

pub fn router(state: SharedState) -> Router {
    Router::new()
        .route("/api/health", get(health))
        .route("/api/projects", get(list_projects).post(create_project))
        .route("/api/projects/:project_id", delete(delete_project))
        .route(
            "/api/projects/:project_id/articles",
            get(list_project_articles).post(create_project_article),
        )
        .route(
            "/api/projects/:project_id/articles/bulk",
            post(bulk_create_project_articles),
        )
        .route(
            "/api/projects/:project_id/articles/:id",
            get(get_project_article)
                .patch(update_project_article)
                .delete(delete_project_article),
        )
        .route("/api/articles", get(list_articles).post(create_article))
        .route(
            "/api/articles/:id",
            get(get_article)
                .patch(update_article)
                .delete(delete_article),
        )
        .route("/api/articles/bulk", post(bulk_create))
        .with_state(state)
}

async fn health() -> &'static str {
    "ok"
}

async fn list_projects(State(state): State<SharedState>) -> AppResult<Json<Vec<Project>>> {
    let store = state.store.read().await;
    Ok(Json(store.list_projects()))
}

async fn create_project(
    State(state): State<SharedState>,
    Json(payload): Json<NewProject>,
) -> AppResult<Json<Project>> {
    let mut store = state.store.write().await;
    let project = store
        .create_project(payload.name)
        .map_err(AppError::Internal)?;
    Ok(Json(project))
}

async fn delete_project(
    State(state): State<SharedState>,
    Path(project_id): Path<String>,
) -> AppResult<StatusCodeWrap> {
    let mut store = state.store.write().await;
    if store
        .delete_project(&project_id)
        .map_err(AppError::Internal)?
    {
        Ok(StatusCodeWrap(axum::http::StatusCode::NO_CONTENT))
    } else {
        Err(AppError::NotFound)
    }
}

async fn list_articles(State(state): State<SharedState>) -> AppResult<Json<Vec<Article>>> {
    list_articles_in_project(state, DEFAULT_PROJECT_ID).await
}

async fn get_article(
    State(state): State<SharedState>,
    Path(id): Path<String>,
) -> AppResult<Json<Article>> {
    get_article_in_project(state, DEFAULT_PROJECT_ID, &id).await
}

async fn create_article(
    State(state): State<SharedState>,
    Json(payload): Json<NewArticle>,
) -> AppResult<Json<Article>> {
    create_article_in_project(state, DEFAULT_PROJECT_ID, payload).await
}

async fn bulk_create(
    State(state): State<SharedState>,
    Json(payload): Json<Vec<NewArticle>>,
) -> AppResult<Json<Vec<Article>>> {
    bulk_create_in_project(state, DEFAULT_PROJECT_ID, payload).await
}

async fn update_article(
    State(state): State<SharedState>,
    Path(id): Path<String>,
    Json(upd): Json<ArticleUpdate>,
) -> AppResult<Json<Article>> {
    update_article_in_project(state, DEFAULT_PROJECT_ID, &id, upd).await
}

async fn delete_article(
    State(state): State<SharedState>,
    Path(id): Path<String>,
) -> AppResult<StatusCodeWrap> {
    delete_article_in_project(state, DEFAULT_PROJECT_ID, &id).await
}

async fn list_project_articles(
    State(state): State<SharedState>,
    Path(project_id): Path<String>,
) -> AppResult<Json<Vec<Article>>> {
    list_articles_in_project(state, &project_id).await
}

async fn get_project_article(
    State(state): State<SharedState>,
    Path((project_id, id)): Path<(String, String)>,
) -> AppResult<Json<Article>> {
    get_article_in_project(state, &project_id, &id).await
}

async fn create_project_article(
    State(state): State<SharedState>,
    Path(project_id): Path<String>,
    Json(payload): Json<NewArticle>,
) -> AppResult<Json<Article>> {
    create_article_in_project(state, &project_id, payload).await
}

async fn bulk_create_project_articles(
    State(state): State<SharedState>,
    Path(project_id): Path<String>,
    Json(payload): Json<Vec<NewArticle>>,
) -> AppResult<Json<Vec<Article>>> {
    bulk_create_in_project(state, &project_id, payload).await
}

async fn update_project_article(
    State(state): State<SharedState>,
    Path((project_id, id)): Path<(String, String)>,
    Json(upd): Json<ArticleUpdate>,
) -> AppResult<Json<Article>> {
    update_article_in_project(state, &project_id, &id, upd).await
}

async fn delete_project_article(
    State(state): State<SharedState>,
    Path((project_id, id)): Path<(String, String)>,
) -> AppResult<StatusCodeWrap> {
    delete_article_in_project(state, &project_id, &id).await
}

async fn list_articles_in_project(
    state: SharedState,
    project_id: &str,
) -> AppResult<Json<Vec<Article>>> {
    let store = state.store.read().await;
    store.list(project_id).map(Json).ok_or(AppError::NotFound)
}

async fn get_article_in_project(
    state: SharedState,
    project_id: &str,
    id: &str,
) -> AppResult<Json<Article>> {
    let store = state.store.read().await;
    store
        .get(project_id, id)
        .map(Json)
        .ok_or(AppError::NotFound)
}

async fn create_article_in_project(
    state: SharedState,
    project_id: &str,
    payload: NewArticle,
) -> AppResult<Json<Article>> {
    if payload.title.trim().is_empty() {
        return Err(AppError::BadRequest("title is required".into()));
    }
    let mut store = state.store.write().await;
    match store.add(project_id, payload).map_err(AppError::Internal)? {
        Some(AddResult::Created(article)) => Ok(Json(*article)),
        Some(AddResult::Duplicate) => Err(AppError::DuplicateArticle),
        None => Err(AppError::NotFound),
    }
}

async fn bulk_create_in_project(
    state: SharedState,
    project_id: &str,
    payload: Vec<NewArticle>,
) -> AppResult<Json<Vec<Article>>> {
    let mut store = state.store.write().await;
    let mut out = Vec::with_capacity(payload.len());
    for article in payload {
        if article.title.trim().is_empty() {
            continue;
        }
        match store.add(project_id, article).map_err(AppError::Internal)? {
            Some(AddResult::Created(article)) => out.push(*article),
            Some(AddResult::Duplicate) => return Err(AppError::DuplicateArticle),
            None => return Err(AppError::NotFound),
        }
    }
    Ok(Json(out))
}

async fn update_article_in_project(
    state: SharedState,
    project_id: &str,
    id: &str,
    upd: ArticleUpdate,
) -> AppResult<Json<Article>> {
    let mut store = state.store.write().await;
    match store
        .update(project_id, id, upd)
        .map_err(AppError::Internal)?
    {
        UpdateResult::Updated(article) => Ok(Json(article)),
        UpdateResult::Conflict(article) => Err(AppError::Conflict(Box::new(article))),
        UpdateResult::Missing => Err(AppError::NotFound),
    }
}

async fn delete_article_in_project(
    state: SharedState,
    project_id: &str,
    id: &str,
) -> AppResult<StatusCodeWrap> {
    let mut store = state.store.write().await;
    if store.delete(project_id, id).map_err(AppError::Internal)? {
        Ok(StatusCodeWrap(axum::http::StatusCode::NO_CONTENT))
    } else {
        Err(AppError::NotFound)
    }
}

pub struct StatusCodeWrap(pub axum::http::StatusCode);

impl axum::response::IntoResponse for StatusCodeWrap {
    fn into_response(self) -> axum::response::Response {
        self.0.into_response()
    }
}
