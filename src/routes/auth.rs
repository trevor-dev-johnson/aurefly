use axum::{
    extract::State,
    http::HeaderMap,
    routing::get,
    Json, Router,
};
use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::{auth::require_user, error::AppResult, models::user::User, state::AppState};

pub fn router() -> Router<AppState> {
    Router::new().route("/me", get(me))
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

async fn me(State(state): State<AppState>, headers: HeaderMap) -> AppResult<Json<UserResponse>> {
    let user = require_user(&headers, &state).await?;
    Ok(Json(user.into()))
}
