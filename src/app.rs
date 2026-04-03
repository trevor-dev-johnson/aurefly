use axum::{
    extract::Request,
    middleware,
    middleware::Next,
    http::{
        header::{AUTHORIZATION, CONTENT_TYPE},
        HeaderName, HeaderValue, Method,
    },
    response::Response,
    Router,
};
use tower_http::{
    cors::{AllowOrigin, CorsLayer},
    trace::TraceLayer,
};

use crate::{routes, state::AppState};

async fn apply_security_headers(request: Request, next: Next) -> Response {
    let mut response = next.run(request).await;
    let headers = response.headers_mut();
    headers.insert(
        HeaderName::from_static("x-content-type-options"),
        HeaderValue::from_static("nosniff"),
    );
    headers.insert(
        HeaderName::from_static("x-frame-options"),
        HeaderValue::from_static("deny"),
    );
    headers.insert(
        HeaderName::from_static("referrer-policy"),
        HeaderValue::from_static("strict-origin-when-cross-origin"),
    );
    headers.insert(
        HeaderName::from_static("permissions-policy"),
        HeaderValue::from_static("camera=(), microphone=(), geolocation=(), payment=()"),
    );
    headers.insert(
        HeaderName::from_static("strict-transport-security"),
        HeaderValue::from_static("max-age=31536000; includeSubDomains; preload"),
    );

    response
}

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
        .layer(middleware::from_fn(apply_security_headers))
        .layer(TraceLayer::new_for_http())
        .layer(cors)
        .with_state(state)
}
