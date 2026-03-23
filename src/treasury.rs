use std::{
    fs,
    path::{Path, PathBuf},
    sync::Arc,
};

use anyhow::{Context, Result};
use solana_sdk::signature::{read_keypair_file, write_keypair_file, Keypair, Signer};

use crate::solana::UsdcSettlement;

#[derive(Debug, Clone)]
pub struct TreasuryWallet {
    pub keypair: Arc<Keypair>,
    pub wallet_pubkey: String,
    pub usdc_ata: String,
    pub usdc_mint: String,
}

impl TreasuryWallet {
    pub fn load_or_create(path: &str, keypair_json: Option<&str>) -> Result<Self> {
        let keypair = if let Some(keypair_json) = keypair_json {
            read_keypair_from_json(keypair_json)
                .context("failed to read treasury keypair from TREASURY_WALLET_JSON")?
        } else {
            let path = PathBuf::from(path);
            if path.exists() {
                read_keypair(&path).with_context(|| {
                    format!("failed to read treasury keypair from {}", path.display())
                })?
            } else {
                create_keypair_file(&path)?
            }
        };

        let settlement = UsdcSettlement::from_wallet_pubkey(&keypair.pubkey().to_string())
            .map_err(|error| anyhow::Error::msg(error.to_string()))?;

        Ok(Self {
            keypair: Arc::new(keypair),
            wallet_pubkey: settlement.wallet_pubkey,
            usdc_ata: settlement.usdc_ata,
            usdc_mint: settlement.usdc_mint,
        })
    }
}

pub fn load_existing_keypair(path: &str) -> Result<Arc<Keypair>> {
    let path = PathBuf::from(path);
    let keypair = read_keypair(&path)
        .with_context(|| format!("failed to read keypair from {}", path.display()))?;
    Ok(Arc::new(keypair))
}

pub fn load_existing_keypair_from_json(keypair_json: &str) -> Result<Arc<Keypair>> {
    let keypair = read_keypair_from_json(keypair_json)
        .context("failed to parse keypair JSON secret")?;
    Ok(Arc::new(keypair))
}

fn create_keypair_file(path: &Path) -> Result<Keypair> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).with_context(|| {
            format!(
                "failed to create treasury wallet directory {}",
                parent.display()
            )
        })?;
    }

    let keypair = Keypair::new();
    write_keypair_file(&keypair, path)
        .map_err(|error| anyhow::anyhow!(error.to_string()))
        .with_context(|| format!("failed to write treasury keypair to {}", path.display()))?;

    Ok(keypair)
}

fn read_keypair(path: &Path) -> Result<Keypair> {
    read_keypair_file(path).map_err(|error| anyhow::anyhow!(error.to_string()))
}

fn read_keypair_from_json(value: &str) -> Result<Keypair> {
    let secret: Vec<u8> = serde_json::from_str(value).context("keypair secret must be valid JSON")?;
    Keypair::try_from(secret.as_slice()).map_err(|error| anyhow::anyhow!(error.to_string()))
}
