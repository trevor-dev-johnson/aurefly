mod app;
mod auth;
mod clients;
mod config;
mod db;
mod detector;
mod error;
mod models;
mod rate_limit;
mod routes;
mod solana;
mod services;
mod state;
mod treasury;

use anyhow::Context;
use std::time::Duration;
use tokio::net::TcpListener;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

use crate::{
    clients::solana::SolanaRpcClient, config::Config, detector::PaymentDetectorConfig,
    rate_limit::AuthRateLimiter,
    services::invoices,
    state::AppState,
};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();
    init_tracing();

    let config = Config::from_env().context("failed to load configuration")?;
    let pool = db::connect(&config.database_url)
        .await
        .context("failed to connect to postgres")?;
    db::migrate(&pool)
        .await
        .context("failed to run database migrations")?;
    let backfilled_references = invoices::backfill_missing_references(&pool)
        .await
        .context("failed to backfill invoice references")?;
    if backfilled_references > 0 {
        tracing::info!(
            backfilled_references,
            "backfilled missing invoice reference pubkeys"
        );
    }

    let solana = SolanaRpcClient::new(config.solana_rpc_url.clone());
    let redacted_rpc_url = redact_rpc_url(&config.solana_rpc_url);
    let rpc_provider = detect_rpc_provider(&config.solana_rpc_url);
    tracing::info!(rpc_provider, rpc_url = %redacted_rpc_url, "using Solana RPC endpoint");

    tokio::spawn(detector::run(
        pool.clone(),
        solana.clone(),
        PaymentDetectorConfig {
            poll_interval: Duration::from_secs(config.payment_detector_poll_interval_secs),
            signature_limit: config.payment_detector_signature_limit,
        },
    ));

    let auth_rate_limiter = AuthRateLimiter::new(
        config.auth_rate_limit_max_requests,
        Duration::from_secs(config.auth_rate_limit_window_secs),
    );
    let state = AppState::new(pool, solana, auth_rate_limiter);
    let app = app::build(state, config.allowed_origins.clone());
    let listener = TcpListener::bind(config.socket_addr())
        .await
        .context("failed to bind TCP listener")?;

    tracing::info!("listening on http://{}", listener.local_addr()?);

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .context("server exited with error")?;

    Ok(())
}

fn init_tracing() {
    let env_filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("aurefly_backend=info,tower_http=info"));

    tracing_subscriber::registry()
        .with(env_filter)
        .with(tracing_subscriber::fmt::layer())
        .init();
}

async fn shutdown_signal() {
    if tokio::signal::ctrl_c().await.is_ok() {
        tracing::info!("shutdown signal received");
    }
}

fn redact_rpc_url(value: &str) -> String {
    let Some((base, query)) = value.split_once('?') else {
        return value.to_string();
    };

    let redacted_query = query
        .split('&')
        .map(|pair| match pair.split_once('=') {
            Some((key, _)) if key.eq_ignore_ascii_case("api-key") => {
                format!("{key}=REDACTED")
            }
            _ => pair.to_string(),
        })
        .collect::<Vec<_>>()
        .join("&");

    format!("{base}?{redacted_query}")
}

fn detect_rpc_provider(value: &str) -> &'static str {
    if value.contains("helius") {
        "helius"
    } else if value.contains("quicknode") {
        "quicknode"
    } else if value.contains("triton") {
        "triton"
    } else if value.contains("solana.com") {
        "solana_public"
    } else {
        "custom"
    }
}
