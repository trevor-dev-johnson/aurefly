use axum::http::{header::AUTHORIZATION, HeaderMap};

use crate::{
    error::{AppError, AppResult},
    models::user::User,
    services::auth,
    state::AppState,
};

pub fn require_bearer_token(headers: &HeaderMap) -> AppResult<String> {
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

    Ok(token.to_string())
}

pub async fn require_user(headers: &HeaderMap, state: &AppState) -> AppResult<User> {
    let token = require_bearer_token(headers)?;

    auth::user_for_token(&state.pool, &state.supabase_auth, &token)
        .await?
        .ok_or_else(|| AppError::Unauthorized("invalid or expired auth token".to_string()))
}

pub fn user_is_admin(user: &User, state: &AppState) -> bool {
    state
        .admin_emails
        .iter()
        .any(|email| email.eq_ignore_ascii_case(&user.email))
}

pub async fn require_admin(headers: &HeaderMap, state: &AppState) -> AppResult<User> {
    let user = require_user(headers, state).await?;

    if user_is_admin(&user, state) {
        Ok(user)
    } else {
        Err(AppError::Forbidden("admin access required".to_string()))
    }
}
