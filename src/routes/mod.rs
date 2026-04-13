pub mod admin;
pub mod auth;
pub mod health;
pub mod invoices;
pub mod me;
pub mod public;

use axum::Router;

use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .nest("/admin", admin::router())
        .nest("/auth", auth::router())
        .nest("/health", health::router())
        .nest("/me", me::router())
        .nest("/public", public::router())
}
