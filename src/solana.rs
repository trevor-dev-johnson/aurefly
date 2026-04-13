use std::str::FromStr;

use solana_sdk::pubkey::Pubkey;

use crate::error::{AppError, AppResult};

pub const MAINNET_USDC_MINT: &str = "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v";
const TOKEN_PROGRAM_ID: &str = "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA";
const ASSOCIATED_TOKEN_PROGRAM_ID: &str = "ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL";

#[derive(Debug, Clone)]
pub struct UsdcSettlement {
    pub wallet_pubkey: String,
    pub usdc_ata: String,
    pub usdc_mint: String,
}

impl UsdcSettlement {
    pub fn from_wallet_pubkey(wallet_pubkey: &str) -> AppResult<Self> {
        let wallet_pubkey = parse_pubkey(wallet_pubkey, "wallet_pubkey")?;
        let usdc_mint = usdc_mint_pubkey()?;
        let token_program = token_program_pubkey()?;
        let ata_program = associated_token_program_pubkey()?;
        let (usdc_ata, _) = Pubkey::find_program_address(
            &[
                wallet_pubkey.as_ref(),
                token_program.as_ref(),
                usdc_mint.as_ref(),
            ],
            &ata_program,
        );

        Ok(Self {
            wallet_pubkey: wallet_pubkey.to_string(),
            usdc_ata: usdc_ata.to_string(),
            usdc_mint: usdc_mint.to_string(),
        })
    }
}

pub fn parse_pubkey(value: &str, field_name: &str) -> AppResult<Pubkey> {
    Pubkey::from_str(value.trim()).map_err(|_| {
        AppError::Validation(format!("{field_name} must be a valid Solana public key"))
    })
}

fn usdc_mint_pubkey() -> AppResult<Pubkey> {
    Pubkey::from_str(MAINNET_USDC_MINT)
        .map_err(|error| AppError::Internal(anyhow::Error::new(error)))
}

fn token_program_pubkey() -> AppResult<Pubkey> {
    Pubkey::from_str(TOKEN_PROGRAM_ID)
        .map_err(|error| AppError::Internal(anyhow::Error::new(error)))
}

fn associated_token_program_pubkey() -> AppResult<Pubkey> {
    Pubkey::from_str(ASSOCIATED_TOKEN_PROGRAM_ID)
        .map_err(|error| AppError::Internal(anyhow::Error::new(error)))
}

pub fn token_program_id() -> AppResult<Pubkey> {
    token_program_pubkey()
}
