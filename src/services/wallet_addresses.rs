use sqlx::PgPool;
use uuid::Uuid;

use crate::{
    error::{AppError, AppResult},
    models::wallet_address::WalletAddress,
    solana::UsdcSettlement,
};

pub struct CreateWalletAddress {
    pub user_id: Uuid,
    pub wallet_pubkey: String,
    pub label: Option<String>,
}

pub async fn create(pool: &PgPool, input: CreateWalletAddress) -> AppResult<WalletAddress> {
    let wallet_pubkey = input.wallet_pubkey.trim().to_string();
    if wallet_pubkey.is_empty() {
        return Err(AppError::Validation(
            "wallet_pubkey is required".to_string(),
        ));
    }
    let settlement = UsdcSettlement::from_wallet_pubkey(&wallet_pubkey)?;

    let label = input.label.and_then(clean_optional);

    let wallet_address = sqlx::query_as::<_, WalletAddress>(
        r#"
        INSERT INTO wallet_addresses (user_id, wallet_pubkey, usdc_ata, usdc_mint, label)
        VALUES ($1, $2, $3, $4, $5)
        RETURNING id, user_id, wallet_pubkey, usdc_ata, usdc_mint, label, is_active, created_at
        "#,
    )
    .bind(input.user_id)
    .bind(settlement.wallet_pubkey)
    .bind(settlement.usdc_ata)
    .bind(settlement.usdc_mint)
    .bind(label)
    .fetch_one(pool)
    .await?;

    Ok(wallet_address)
}

pub async fn list(pool: &PgPool) -> AppResult<Vec<WalletAddress>> {
    let wallet_addresses = sqlx::query_as::<_, WalletAddress>(
        r#"
        SELECT id, user_id, wallet_pubkey, usdc_ata, usdc_mint, label, is_active, created_at
        FROM wallet_addresses
        ORDER BY created_at DESC
        "#,
    )
    .fetch_all(pool)
    .await?;

    Ok(wallet_addresses)
}

fn clean_optional(value: String) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}
