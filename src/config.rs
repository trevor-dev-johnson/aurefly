use std::{
    env,
    net::{IpAddr, Ipv4Addr, SocketAddr},
};

use anyhow::{Context, Result};

#[derive(Debug, Clone)]
pub struct Config {
    pub database_url: String,
    pub host: IpAddr,
    pub port: u16,
    pub allowed_origins: Vec<String>,
    pub admin_emails: Vec<String>,
    pub supabase_url: Option<String>,
    pub supabase_publishable_key: Option<String>,
    pub solana_rpc_url: String,
    pub solana_fallback_rpc_url: Option<String>,
    pub solana_fallback_ws_url: Option<String>,
    pub treasury_wallet_path: String,
    pub treasury_wallet_json: Option<String>,
    pub solana_fee_payer_path: Option<String>,
    pub solana_fee_payer_json: Option<String>,
    pub payment_detector_poll_interval_secs: u64,
    pub payment_detector_fast_poll_interval_secs: u64,
    pub payment_detector_medium_poll_interval_secs: u64,
    pub payment_detector_slow_poll_interval_secs: u64,
    pub payment_detector_fast_window_secs: u64,
    pub payment_detector_medium_window_secs: u64,
    pub payment_detector_max_targets_per_cycle: usize,
    pub payment_detector_max_active_logs_subscriptions: usize,
    pub payment_detector_max_idle_backoff_secs: u64,
    pub payment_detector_signature_dedupe_ttl_secs: u64,
    pub payment_detector_signature_limit: usize,
    pub invoice_pending_ttl_secs: u64,
}

impl Config {
    pub fn from_env() -> Result<Self> {
        let database_url = env::var("DATABASE_URL").context("DATABASE_URL is required")?;
        let host = env::var("HOST")
            .unwrap_or_else(|_| Ipv4Addr::UNSPECIFIED.to_string())
            .parse()
            .context("HOST must be a valid IP address")?;
        let port = env::var("PORT")
            .unwrap_or_else(|_| "8080".to_string())
            .parse()
            .context("PORT must be a valid u16")?;
        let allowed_origins = optional_env("ALLOWED_ORIGINS")
            .unwrap_or_else(|| {
                "https://aurefly.com,https://www.aurefly.com,http://localhost:3000,http://127.0.0.1:3000"
                    .to_string()
            })
            .split(',')
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToString::to_string)
            .collect();
        let admin_emails = optional_env("ADMIN_EMAILS")
            .unwrap_or_default()
            .split(',')
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(|value| value.to_lowercase())
            .collect();
        let supabase_url = optional_env_any(&["SUPABASE_URL", "NEXT_PUBLIC_SUPABASE_URL"]);
        let supabase_publishable_key = optional_env_any(&[
            "SUPABASE_PUBLISHABLE_KEY",
            "SUPABASE_ANON_KEY",
            "SUPABASE_PUBLISHABLE_DEFAULT_KEY",
            "NEXT_PUBLIC_SUPABASE_PUBLISHABLE_DEFAULT_KEY",
        ]);
        let helius_api_key = env::var("HELIUS_API_KEY").ok().and_then(|value| {
            let trimmed = value.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            }
        });
        let solana_rpc_url = env::var("SOLANA_RPC_URL")
            .ok()
            .and_then(|value| {
                let trimmed = value.trim();
                if trimmed.is_empty() {
                    None
                } else {
                    Some(trimmed.to_string())
                }
            })
            .or_else(|| {
                helius_api_key
                    .map(|api_key| format!("https://mainnet.helius-rpc.com/?api-key={api_key}"))
            })
            .unwrap_or_else(|| "https://api.mainnet-beta.solana.com".to_string());
        let solana_fallback_rpc_url = optional_env("SOLANA_FALLBACK_RPC_URL");
        let solana_fallback_ws_url = optional_env("SOLANA_FALLBACK_WS_URL");
        let treasury_wallet_path = env::var("TREASURY_WALLET_PATH")
            .unwrap_or_else(|_| "./data/treasury-wallet.json".to_string());
        let treasury_wallet_json = optional_env("TREASURY_WALLET_JSON");
        let solana_fee_payer_path = env::var("SOLANA_FEE_PAYER_PATH").ok().and_then(|value| {
            let trimmed = value.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            }
        });
        let solana_fee_payer_json = optional_env("SOLANA_FEE_PAYER_JSON");
        let payment_detector_poll_interval_secs = env::var("PAYMENT_DETECTOR_POLL_INTERVAL_SECS")
            .or_else(|_| env::var("PAYMENT_DETECTOR_SCHEDULER_TICK_SECS"))
            .unwrap_or_else(|_| "5".to_string())
            .parse()
            .context("PAYMENT_DETECTOR_POLL_INTERVAL_SECS must be a valid u64")?;
        let payment_detector_fast_poll_interval_secs =
            env::var("PAYMENT_DETECTOR_FAST_POLL_INTERVAL_SECS")
                .unwrap_or_else(|_| "6".to_string())
                .parse()
                .context("PAYMENT_DETECTOR_FAST_POLL_INTERVAL_SECS must be a valid u64")?;
        let payment_detector_medium_poll_interval_secs =
            env::var("PAYMENT_DETECTOR_MEDIUM_POLL_INTERVAL_SECS")
                .unwrap_or_else(|_| "20".to_string())
                .parse()
                .context("PAYMENT_DETECTOR_MEDIUM_POLL_INTERVAL_SECS must be a valid u64")?;
        let payment_detector_slow_poll_interval_secs =
            env::var("PAYMENT_DETECTOR_SLOW_POLL_INTERVAL_SECS")
                .unwrap_or_else(|_| "60".to_string())
                .parse()
                .context("PAYMENT_DETECTOR_SLOW_POLL_INTERVAL_SECS must be a valid u64")?;
        let payment_detector_fast_window_secs = env::var("PAYMENT_DETECTOR_FAST_WINDOW_SECS")
            .unwrap_or_else(|_| "120".to_string())
            .parse()
            .context("PAYMENT_DETECTOR_FAST_WINDOW_SECS must be a valid u64")?;
        let payment_detector_medium_window_secs = env::var("PAYMENT_DETECTOR_MEDIUM_WINDOW_SECS")
            .unwrap_or_else(|_| "900".to_string())
            .parse()
            .context("PAYMENT_DETECTOR_MEDIUM_WINDOW_SECS must be a valid u64")?;
        let payment_detector_max_targets_per_cycle =
            env::var("PAYMENT_DETECTOR_MAX_TARGETS_PER_CYCLE")
                .unwrap_or_else(|_| "6".to_string())
                .parse()
                .context("PAYMENT_DETECTOR_MAX_TARGETS_PER_CYCLE must be a valid usize")?;
        let payment_detector_max_active_logs_subscriptions =
            env::var("PAYMENT_DETECTOR_MAX_ACTIVE_LOGS_SUBSCRIPTIONS")
                .unwrap_or_else(|_| "12".to_string())
                .parse()
                .context("PAYMENT_DETECTOR_MAX_ACTIVE_LOGS_SUBSCRIPTIONS must be a valid usize")?;
        let payment_detector_max_idle_backoff_secs =
            env::var("PAYMENT_DETECTOR_MAX_IDLE_BACKOFF_SECS")
                .unwrap_or_else(|_| "300".to_string())
                .parse()
                .context("PAYMENT_DETECTOR_MAX_IDLE_BACKOFF_SECS must be a valid u64")?;
        let payment_detector_signature_dedupe_ttl_secs =
            env::var("PAYMENT_DETECTOR_SIGNATURE_DEDUPE_TTL_SECS")
                .unwrap_or_else(|_| "300".to_string())
                .parse()
                .context("PAYMENT_DETECTOR_SIGNATURE_DEDUPE_TTL_SECS must be a valid u64")?;
        let payment_detector_signature_limit = env::var("PAYMENT_DETECTOR_SIGNATURE_LIMIT")
            .unwrap_or_else(|_| "25".to_string())
            .parse()
            .context("PAYMENT_DETECTOR_SIGNATURE_LIMIT must be a valid usize")?;
        let invoice_pending_ttl_secs = env::var("INVOICE_PENDING_TTL_SECS")
            .unwrap_or_else(|_| "1800".to_string())
            .parse()
            .context("INVOICE_PENDING_TTL_SECS must be a valid u64")?;

        Ok(Self {
            database_url,
            host,
            port,
            allowed_origins,
            admin_emails,
            supabase_url,
            supabase_publishable_key,
            solana_rpc_url,
            solana_fallback_rpc_url,
            solana_fallback_ws_url,
            treasury_wallet_path,
            treasury_wallet_json,
            solana_fee_payer_path,
            solana_fee_payer_json,
            payment_detector_poll_interval_secs,
            payment_detector_fast_poll_interval_secs,
            payment_detector_medium_poll_interval_secs,
            payment_detector_slow_poll_interval_secs,
            payment_detector_fast_window_secs,
            payment_detector_medium_window_secs,
            payment_detector_max_targets_per_cycle,
            payment_detector_max_active_logs_subscriptions,
            payment_detector_max_idle_backoff_secs,
            payment_detector_signature_dedupe_ttl_secs,
            payment_detector_signature_limit,
            invoice_pending_ttl_secs,
        })
    }

    pub fn socket_addr(&self) -> SocketAddr {
        SocketAddr::new(self.host, self.port)
    }
}

fn optional_env(name: &str) -> Option<String> {
    env::var(name).ok().and_then(|value| {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    })
}

fn optional_env_any(names: &[&str]) -> Option<String> {
    names.iter().find_map(|name| optional_env(name))
}
