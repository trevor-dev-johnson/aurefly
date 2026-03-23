use axum::{
    extract::State,
    http::StatusCode,
    routing::post,
    Json, Router,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    error::AppResult,
    models::user::User,
    services::users::{self, CreateUser},
    state::AppState,
};

pub fn router() -> Router<AppState> {
    Router::new().route("/", post(create_user).get(list_users))
}

#[derive(Debug, Deserialize)]
struct CreateUserRequest {
    email: String,
    name: Option<String>,
}

#[derive(Debug, Serialize)]
pub(crate) struct UserResponse {
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

async fn create_user(
    State(state): State<AppState>,
    Json(payload): Json<CreateUserRequest>,
) -> AppResult<(StatusCode, Json<UserResponse>)> {
    let user = users::create(
        &state.pool,
        CreateUser {
            email: payload.email,
            name: payload.name,
        },
    )
    .await?;

    Ok((StatusCode::CREATED, Json(user.into())))
}

async fn list_users(State(state): State<AppState>) -> AppResult<Json<Vec<UserResponse>>> {
    let users = users::list(&state.pool).await?;
    let response = users.into_iter().map(UserResponse::from).collect();
    Ok(Json(response))
}
