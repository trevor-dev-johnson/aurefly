pub mod auth;
pub mod health;
pub mod invoices;
pub mod me;
pub mod payments;
pub mod public;
pub mod users;
pub mod wallet_addresses;

use axum::Router;

use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .nest("/auth", auth::router())
        .nest("/health", health::router())
        .nest("/me", me::router())
        .nest("/public", public::router())
        .nest("/users", users::router())
        .nest("/wallet-addresses", wallet_addresses::router())
        .nest("/invoices", invoices::router())
        .nest("/payments", payments::router())
}
