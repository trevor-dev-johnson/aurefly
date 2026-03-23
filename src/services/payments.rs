use std::str::FromStr;

use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use sqlx::PgPool;
use uuid::Uuid;

use crate::{
    error::{AppError, AppResult},
    models::payment::Payment,
};

pub struct CreatePayment {
    pub invoice_id: Uuid,
    pub amount_usdc: String,
    pub tx_signature: String,
    pub payer_wallet_address: Option<String>,
    pub finalized_at: Option<DateTime<Utc>>,
    pub slot: Option<i64>,
}

pub struct CreatePaymentResult {
    pub payment: Payment,
    pub inserted: bool,
}

pub async fn create(pool: &PgPool, input: CreatePayment) -> AppResult<CreatePaymentResult> {
    let amount = parse_amount(&input.amount_usdc)?;
    let tx_signature = input.tx_signature.trim().to_string();

    if tx_signature.is_empty() {
        return Err(AppError::Validation(
            "tx_signature is required".to_string(),
        ));
    }

    let payer_wallet_address = input.payer_wallet_address.and_then(clean_optional);
    let mut transaction = pool.begin().await?;

    let invoice = sqlx::query_as::<_, InvoicePaymentTarget>(
        r#"
        SELECT amount_usdc, usdc_ata, usdc_mint
        FROM invoices
        WHERE id = $1
        FOR UPDATE
        "#,
    )
    .bind(input.invoice_id)
    .fetch_one(&mut *transaction)
    .await?;

    let payment = sqlx::query_as::<_, Payment>(
        r#"
        INSERT INTO payments (
            invoice_id,
            amount_usdc,
            status,
            tx_signature,
            payer_wallet_address,
            recipient_token_account,
            token_mint,
            finalized_at,
            slot
        )
        VALUES ($1, $2, 'confirmed', $3, $4, $5, $6, $7, $8)
        ON CONFLICT (tx_signature) DO NOTHING
        RETURNING
            id,
            invoice_id,
            amount_usdc,
            status,
            tx_signature,
            payer_wallet_address,
            recipient_token_account,
            token_mint,
            finalized_at,
            slot,
            created_at
        "#,
    )
    .bind(input.invoice_id)
    .bind(amount)
    .bind(&tx_signature)
    .bind(payer_wallet_address)
    .bind(&invoice.usdc_ata)
    .bind(&invoice.usdc_mint)
    .bind(input.finalized_at)
    .bind(input.slot)
    .fetch_optional(&mut *transaction)
    .await?;

    let Some(payment) = payment else {
        transaction.rollback().await?;

        let existing = get_by_tx_signature(pool, &tx_signature)
            .await?
            .ok_or_else(|| {
                AppError::Internal(anyhow::anyhow!(
                    "payment insert conflicted but no existing tx_signature record was found"
                ))
            })?;

        if existing.invoice_id != input.invoice_id {
            return Err(AppError::Validation(
                "tx_signature already exists for a different invoice".to_string(),
            ));
        }

        return Ok(CreatePaymentResult {
            payment: existing,
            inserted: false,
        });
    };

    let confirmed_total = sqlx::query_scalar::<_, Decimal>(
        r#"
        SELECT COALESCE(SUM(amount_usdc), 0::numeric)
        FROM payments
        WHERE invoice_id = $1 AND status = 'confirmed'
        "#,
    )
    .bind(input.invoice_id)
    .fetch_one(&mut *transaction)
    .await?;

    if confirmed_total >= invoice.amount_usdc {
        sqlx::query(
            r#"
            UPDATE invoices
            SET status = 'paid', paid_at = NOW()
            WHERE id = $1
            "#,
        )
        .bind(input.invoice_id)
        .execute(&mut *transaction)
        .await?;
    }

    transaction.commit().await?;

    Ok(CreatePaymentResult {
        payment,
        inserted: true,
    })
}

pub async fn list(pool: &PgPool) -> AppResult<Vec<Payment>> {
    let payments = sqlx::query_as::<_, Payment>(
        r#"
        SELECT
            id,
            invoice_id,
            amount_usdc,
            status,
            tx_signature,
            payer_wallet_address,
            recipient_token_account,
            token_mint,
            finalized_at,
            slot,
            created_at
        FROM payments
        ORDER BY created_at DESC
        "#,
    )
    .fetch_all(pool)
    .await?;

    Ok(payments)
}

pub async fn tx_signature_exists(pool: &PgPool, tx_signature: &str) -> AppResult<bool> {
    let exists = sqlx::query_scalar::<_, bool>(
        r#"
        SELECT EXISTS(
            SELECT 1
            FROM payments
            WHERE tx_signature = $1
        )
        "#,
    )
    .bind(tx_signature)
    .fetch_one(pool)
    .await?;

    Ok(exists)
}

async fn get_by_tx_signature(pool: &PgPool, tx_signature: &str) -> AppResult<Option<Payment>> {
    let payment = sqlx::query_as::<_, Payment>(
        r#"
        SELECT
            id,
            invoice_id,
            amount_usdc,
            status,
            tx_signature,
            payer_wallet_address,
            recipient_token_account,
            token_mint,
            finalized_at,
            slot,
            created_at
        FROM payments
        WHERE tx_signature = $1
        "#,
    )
    .bind(tx_signature)
    .fetch_optional(pool)
    .await?;

    Ok(payment)
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

#[derive(sqlx::FromRow)]
struct InvoicePaymentTarget {
    amount_usdc: Decimal,
    usdc_ata: String,
    usdc_mint: String,
}
