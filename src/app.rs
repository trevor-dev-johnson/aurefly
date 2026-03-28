use axum::{
    http::{
        header::{AUTHORIZATION, CONTENT_TYPE},
        HeaderValue, Method,
    },
    Router,
};
use tower_http::{
    cors::{AllowOrigin, CorsLayer},
    trace::TraceLayer,
};

use crate::{routes, state::AppState};

pub fn build(state: AppState, allowed_origins: Vec<String>) -> Router {
    let allowed_origins = allowed_origins
        .into_iter()
        .filter_map(|origin| match HeaderValue::from_str(&origin) {
            Ok(origin) => Some(origin),
            Err(error) => {
                tracing::warn!(%origin, %error, "ignoring invalid CORS origin");
                None
            }
        })
        .collect::<Vec<_>>();

    let cors = CorsLayer::new()
        .allow_methods([Method::GET, Method::POST, Method::OPTIONS])
        .allow_headers([AUTHORIZATION, CONTENT_TYPE]);

    let cors = if allowed_origins.is_empty() {
        cors
    } else {
        cors.allow_origin(AllowOrigin::list(allowed_origins))
    };

    Router::new()
        .nest("/api/v1", routes::router())
        .layer(TraceLayer::new_for_http())
        .layer(cors)
        .with_state(state)
}
