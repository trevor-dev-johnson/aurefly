use std::str::FromStr;

use rust_decimal::Decimal;
use sqlx::PgPool;

use crate::error::{AppError, AppResult};

pub struct CreateUnmatchedPayment {
    pub signature: String,
    pub destination_wallet: String,
    pub amount_usdc: String,
    pub sender_wallet: Option<String>,
    pub reference_pubkey: Option<String>,
    pub reason: String,
}

pub async fn create(pool: &PgPool, input: CreateUnmatchedPayment) -> AppResult<bool> {
    let signature = input.signature.trim().to_string();
    if signature.is_empty() {
        return Err(AppError::Validation(
            "signature is required".to_string(),
        ));
    }

    let destination_wallet = input.destination_wallet.trim().to_string();
    if destination_wallet.is_empty() {
        return Err(AppError::Validation(
            "destination_wallet is required".to_string(),
        ));
    }

    let reason = input.reason.trim().to_string();
    if reason.is_empty() {
        return Err(AppError::Validation("reason is required".to_string()));
    }

    let amount = parse_amount(&input.amount_usdc)?;
    let sender_wallet = input.sender_wallet.and_then(clean_optional);
    let reference_pubkey = input.reference_pubkey.and_then(clean_optional);

    let result = sqlx::query(
        r#"
        INSERT INTO unmatched_payments (
            signature,
            destination_wallet,
            amount_usdc,
            sender_wallet,
            reference_pubkey,
            reason,
            status
        )
        VALUES ($1, $2, $3, $4, $5, $6, 'pending')
        ON CONFLICT (signature) DO NOTHING
        "#,
    )
    .bind(signature)
    .bind(destination_wallet)
    .bind(amount)
    .bind(sender_wallet)
    .bind(reference_pubkey)
    .bind(reason)
    .execute(pool)
    .await?;

    Ok(result.rows_affected() > 0)
}

fn clean_optional(value: String) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

fn parse_amount(raw_amount: &str) -> AppResult<Decimal> {
    let amount = Decimal::from_str(raw_amount.trim())
        .map_err(|_| AppError::Validation("amount_usdc must be a valid decimal string".to_string()))?;

    if amount <= Decimal::ZERO {
        return Err(AppError::Validation(
            "amount_usdc must be greater than zero".to_string(),
        ));
    }

    Ok(amount.round_dp(6))
}
