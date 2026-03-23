use axum::{response::Html, routing::get, Router};
use tower_http::{cors::CorsLayer, services::ServeDir, trace::TraceLayer};

use crate::{routes, state::AppState};

pub fn build(state: AppState) -> Router {
    Router::new()
        .nest("/api/v1", routes::router())
        .route("/", get(index_page))
        .route("/pay/{invoice_id}", get(invoice_page))
        .nest_service("/static", ServeDir::new("public"))
        .layer(TraceLayer::new_for_http())
        .layer(CorsLayer::permissive())
        .with_state(state)
}

async fn index_page() -> Html<&'static str> {
    Html(include_str!("../public/index.html"))
}

async fn invoice_page() -> Html<&'static str> {
    Html(include_str!("../public/invoice.html"))
}
