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
    pub solana_rpc_url: String,
    pub treasury_wallet_path: String,
    pub treasury_wallet_json: Option<String>,
    pub solana_fee_payer_path: Option<String>,
    pub solana_fee_payer_json: Option<String>,
    pub auth_rate_limit_max_requests: usize,
    pub auth_rate_limit_window_secs: u64,
    pub payment_detector_poll_interval_secs: u64,
    pub payment_detector_signature_limit: usize,
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
                helius_api_key.map(|api_key| {
                    format!("https://mainnet.helius-rpc.com/?api-key={api_key}")
                })
            })
            .unwrap_or_else(|| "https://api.mainnet-beta.solana.com".to_string());
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
        let auth_rate_limit_max_requests = env::var("AUTH_RATE_LIMIT_MAX_REQUESTS")
            .unwrap_or_else(|_| "10".to_string())
            .parse()
            .context("AUTH_RATE_LIMIT_MAX_REQUESTS must be a valid usize")?;
        let auth_rate_limit_window_secs = env::var("AUTH_RATE_LIMIT_WINDOW_SECS")
            .unwrap_or_else(|_| "60".to_string())
            .parse()
            .context("AUTH_RATE_LIMIT_WINDOW_SECS must be a valid u64")?;
        let payment_detector_poll_interval_secs = env::var("PAYMENT_DETECTOR_POLL_INTERVAL_SECS")
            .unwrap_or_else(|_| "10".to_string())
            .parse()
            .context("PAYMENT_DETECTOR_POLL_INTERVAL_SECS must be a valid u64")?;
        let payment_detector_signature_limit = env::var("PAYMENT_DETECTOR_SIGNATURE_LIMIT")
            .unwrap_or_else(|_| "25".to_string())
            .parse()
            .context("PAYMENT_DETECTOR_SIGNATURE_LIMIT must be a valid usize")?;

        Ok(Self {
            database_url,
            host,
            port,
            allowed_origins,
            solana_rpc_url,
            treasury_wallet_path,
            treasury_wallet_json,
            solana_fee_payer_path,
            solana_fee_payer_json,
            auth_rate_limit_max_requests,
            auth_rate_limit_window_secs,
            payment_detector_poll_interval_secs,
            payment_detector_signature_limit,
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
