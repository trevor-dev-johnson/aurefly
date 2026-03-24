use axum::{
    extract::{State},
    http::HeaderMap,
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use chrono::{DateTime, Utc};
use serde::Deserialize;
use uuid::Uuid;

use crate::{
    auth::{require_bearer_token, require_user},
    error::AppResult,
    services::auth::{self, RegisterUser, SignInUser},
    models::user::User,
    state::AppState,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/sign-up", post(sign_up))
        .route("/sign-in", post(sign_in))
        .route("/logout", post(log_out))
        .route("/me", get(me))
}

#[derive(Debug, Deserialize)]
struct SignUpRequest {
    email: String,
    password: String,
    name: Option<String>,
}

#[derive(Debug, Deserialize)]
struct SignInRequest {
    email: String,
    password: String,
}

#[derive(serde::Serialize)]
struct AuthResponse {
    token: String,
    user: UserResponse,
}

#[derive(Debug, serde::Serialize)]
struct UserResponse {
    id: Uuid,
    email: String,
    name: Option<String>,
    created_at: DateTime<Utc>,
}

impl From<User> for UserResponse {
    fn from(user: User) -> Self {
        Self {
            id: user.id,
            email: user.email,
            name: user.name,
            created_at: user.created_at,
        }
    }
}

async fn sign_up(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<SignUpRequest>,
) -> AppResult<(StatusCode, Json<AuthResponse>)> {
    enforce_auth_rate_limit(&state, &headers, "sign-up").await?;
    let session = auth::register(
        &state.pool,
        RegisterUser {
            email: payload.email,
            password: payload.password,
            name: payload.name,
        },
    )
    .await?;

    Ok((
        StatusCode::CREATED,
        Json(AuthResponse {
            token: session.token,
            user: session.user.into(),
        }),
    ))
}

async fn sign_in(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<SignInRequest>,
) -> AppResult<Json<AuthResponse>> {
    enforce_auth_rate_limit(&state, &headers, "sign-in").await?;
    let session = auth::sign_in(
        &state.pool,
        SignInUser {
            email: payload.email,
            password: payload.password,
        },
    )
    .await?;

    Ok(Json(AuthResponse {
        token: session.token,
        user: session.user.into(),
    }))
}

async fn me(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> AppResult<Json<UserResponse>> {
    let user = require_user(&headers, &state).await?;
    Ok(Json(user.into()))
}

async fn log_out(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> AppResult<StatusCode> {
    let token = require_bearer_token(&headers)?;
    auth::revoke_token(&state.pool, &token).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn enforce_auth_rate_limit(
    state: &AppState,
    headers: &HeaderMap,
    operation: &str,
) -> AppResult<()> {
    let client_ip = client_ip(headers);
    let key = format!("auth:{operation}:{client_ip}");
    state.auth_rate_limiter.check(&key, operation).await
}

fn client_ip(headers: &HeaderMap) -> String {
    headers
        .get("x-forwarded-for")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.split(',').next())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .or_else(|| {
            headers
                .get("x-real-ip")
                .and_then(|value| value.to_str().ok())
                .map(str::trim)
                .filter(|value| !value.is_empty())
        })
        .unwrap_or("unknown")
        .to_string()
}
