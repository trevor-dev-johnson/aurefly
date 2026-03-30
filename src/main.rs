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
    clients::{solana::SolanaRpcClient, supabase::SupabaseAuthClient},
    config::Config, detector::PaymentDetectorConfig,
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

    let solana = SolanaRpcClient::new(
        config.solana_rpc_url.clone(),
        config.solana_fallback_rpc_url.clone(),
        config.solana_fallback_ws_url.clone(),
    );
    let redacted_rpc_url = solana.redacted_rpc_url();
    let redacted_fallback_rpc_url = solana.redacted_fallback_rpc_url();
    let rpc_provider = detect_rpc_provider(&config.solana_rpc_url);
    let fallback_rpc_provider = config
        .solana_fallback_rpc_url
        .as_deref()
        .map(detect_rpc_provider)
        .unwrap_or("none");
    tracing::info!(
        rpc_provider,
        rpc_url = %redacted_rpc_url,
        fallback_rpc_provider,
        fallback_rpc_url = ?redacted_fallback_rpc_url,
        "using Solana RPC endpoint"
    );
    let supabase_auth = SupabaseAuthClient::new(
        config.supabase_url.clone(),
        config.supabase_publishable_key.clone(),
    );
    tracing::info!(
        configured = supabase_auth.is_configured(),
        supabase_url = ?supabase_auth.redacted_supabase_url(),
        "using Supabase auth provider"
    );

    let detector_poll_interval = Duration::from_secs(config.payment_detector_poll_interval_secs);
    let detector_signature_limit = config.payment_detector_signature_limit;
    let invoice_pending_ttl = Duration::from_secs(config.invoice_pending_ttl_secs);
    tracing::info!(
        poll_interval_secs = detector_poll_interval.as_secs(),
        signature_limit = detector_signature_limit,
        pending_invoice_ttl_secs = invoice_pending_ttl.as_secs(),
        websocket_enabled = solana.websocket_url().is_some(),
        "starting payment detector"
    );

    let detector_handle = tokio::spawn(detector::run(
        pool.clone(),
        solana.clone(),
        PaymentDetectorConfig {
            poll_interval: detector_poll_interval,
            signature_limit: detector_signature_limit,
            pending_invoice_ttl: invoice_pending_ttl,
        },
    ));
    tokio::spawn(async move {
        match detector_handle.await {
            Ok(()) => tracing::error!("payment detector exited unexpectedly"),
            Err(error) => tracing::error!(error = %error, "payment detector task crashed"),
        }
    });

    let state = AppState::new(pool, solana, supabase_auth);
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
