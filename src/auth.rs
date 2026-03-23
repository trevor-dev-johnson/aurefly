use axum::http::{header::AUTHORIZATION, HeaderMap};

use crate::{
    error::{AppError, AppResult},
    models::user::User,
    services::auth,
    state::AppState,
};

pub async fn require_user(headers: &HeaderMap, state: &AppState) -> AppResult<User> {
    let authorization = headers
        .get(AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .ok_or_else(|| AppError::Unauthorized("missing bearer token".to_string()))?;
    let token = authorization
        .strip_prefix("Bearer ")
        .ok_or_else(|| AppError::Unauthorized("invalid authorization scheme".to_string()))?
        .trim();

    if token.is_empty() {
        return Err(AppError::Unauthorized("missing bearer token".to_string()));
    }

    auth::user_for_token(&state.pool, token)
        .await?
        .ok_or_else(|| AppError::Unauthorized("invalid or expired auth token".to_string()))
}
