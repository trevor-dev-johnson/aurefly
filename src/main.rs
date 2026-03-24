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
use solana_sdk::signer::Signer;
use tokio::net::TcpListener;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

use crate::{
    clients::solana::SolanaRpcClient, config::Config, detector::PaymentDetectorConfig,
    rate_limit::AuthRateLimiter,
    services::invoices,
    state::AppState,
    treasury::{load_existing_keypair, load_existing_keypair_from_json, TreasuryWallet},
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

    let treasury = TreasuryWallet::load_or_create(
        &config.treasury_wallet_path,
        config.treasury_wallet_json.as_deref(),
    )
        .context("failed to load treasury wallet")?;
    let solana = SolanaRpcClient::new(config.solana_rpc_url.clone());
    let redacted_rpc_url = redact_rpc_url(&config.solana_rpc_url);
    let rpc_provider = detect_rpc_provider(&config.solana_rpc_url);
    tracing::info!(rpc_provider, rpc_url = %redacted_rpc_url, "using Solana RPC endpoint");
    let fee_payer = if let Some(keypair_json) = config.solana_fee_payer_json.as_deref() {
        let fee_payer = load_existing_keypair_from_json(keypair_json)
            .context("failed to load Solana fee payer")?;
        tracing::info!(
            fee_payer = %fee_payer.pubkey(),
            "loaded Solana fee payer from SOLANA_FEE_PAYER_JSON for ATA creation"
        );
        fee_payer
    } else if let Some(path) = config.solana_fee_payer_path.as_deref() {
        let fee_payer = load_existing_keypair(path).context("failed to load Solana fee payer")?;
        tracing::info!(
            fee_payer = %fee_payer.pubkey(),
            fee_payer_path = %path,
            "loaded Solana fee payer for ATA creation"
        );
        fee_payer
    } else {
        tracing::info!(
            fee_payer = %treasury.wallet_pubkey,
            "using treasury wallet as ATA creation fee payer"
        );
        treasury.keypair.clone()
    };

    if let Some(signature) = solana
        .ensure_associated_token_account(&treasury, fee_payer.as_ref())
        .await
        .context("failed to ensure treasury USDC ATA exists")?
    {
        tracing::info!(
            wallet_pubkey = %treasury.wallet_pubkey,
            usdc_ata = %treasury.usdc_ata,
            fee_payer = %fee_payer.pubkey(),
            rpc_provider,
            rpc_url = %redacted_rpc_url,
            tx_signature = %signature,
            "created treasury USDC ATA"
        );
    } else {
        tracing::info!(
            wallet_pubkey = %treasury.wallet_pubkey,
            usdc_ata = %treasury.usdc_ata,
            fee_payer = %fee_payer.pubkey(),
            rpc_provider,
            rpc_url = %redacted_rpc_url,
            "treasury wallet and USDC ATA are ready"
        );
    }

    tokio::spawn(detector::run(
        pool.clone(),
        solana.clone(),
        PaymentDetectorConfig {
            poll_interval: Duration::from_secs(config.payment_detector_poll_interval_secs),
            match_window: chrono::Duration::seconds(config.invoice_match_window_secs),
            signature_limit: config.payment_detector_signature_limit,
        },
    ));

    let auth_rate_limiter = AuthRateLimiter::new(
        config.auth_rate_limit_max_requests,
        Duration::from_secs(config.auth_rate_limit_window_secs),
    );
    let state = AppState::new(pool, solana, treasury, auth_rate_limiter);
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
