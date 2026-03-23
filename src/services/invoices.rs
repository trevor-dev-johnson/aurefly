use std::str::FromStr;

use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use solana_sdk::{hash::hash, pubkey::Pubkey};
use sqlx::PgPool;
use uuid::Uuid;

use crate::{
    error::{AppError, AppResult},
    models::invoice::Invoice,
    treasury::TreasuryWallet,
};

pub const PLATFORM_FEE_BPS: i16 = 100;

pub struct CreateInvoice {
    pub user_id: Uuid,
    pub amount_usdc: String,
    pub description: Option<String>,
    pub client_email: Option<String>,
}

pub async fn create(
    pool: &PgPool,
    treasury: &TreasuryWallet,
    input: CreateInvoice,
) -> AppResult<Invoice> {
    let invoice_id = Uuid::new_v4();
    let subtotal = parse_amount(&input.amount_usdc)?;
    let platform_fee = calculate_platform_fee(subtotal);
    let amount = subtotal + platform_fee;
    let reference_pubkey = invoice_reference_pubkey(invoice_id);
    let description = clean_optional(input.description);
    let client_email = clean_optional(input.client_email);

    let invoice = sqlx::query_as::<_, Invoice>(
        r#"
        INSERT INTO invoices (
            id,
            user_id,
            reference_pubkey,
            subtotal_usdc,
            platform_fee_usdc,
            platform_fee_bps,
            amount_usdc,
            description,
            client_email,
            status,
            wallet_address,
            wallet_pubkey,
            usdc_ata,
            usdc_mint
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, 'pending', $10, $11, $12, $13)
        RETURNING
            id,
            user_id,
            reference_pubkey,
            subtotal_usdc,
            platform_fee_usdc,
            platform_fee_bps,
            amount_usdc,
            description,
            client_email,
            status,
            wallet_pubkey,
            usdc_ata,
            usdc_mint,
            paid_at,
            0::numeric AS paid_amount_usdc,
            NULL::text AS latest_payment_tx_signature,
            created_at
        "#,
    )
    .bind(invoice_id)
    .bind(input.user_id)
    .bind(&reference_pubkey)
    .bind(subtotal)
    .bind(platform_fee)
    .bind(PLATFORM_FEE_BPS)
    .bind(amount)
    .bind(&description)
    .bind(&client_email)
    .bind(&treasury.usdc_ata)
    .bind(&treasury.wallet_pubkey)
    .bind(&treasury.usdc_ata)
    .bind(&treasury.usdc_mint)
    .fetch_one(pool)
    .await?;

    Ok(invoice)
}

pub async fn list(pool: &PgPool) -> AppResult<Vec<Invoice>> {
    let invoices = sqlx::query_as::<_, Invoice>(
        r#"
        SELECT
            invoices.id,
            invoices.user_id,
            invoices.reference_pubkey,
            invoices.subtotal_usdc,
            invoices.platform_fee_usdc,
            invoices.platform_fee_bps,
            invoices.amount_usdc,
            invoices.description,
            invoices.client_email,
            invoices.status,
            invoices.wallet_pubkey,
            invoices.usdc_ata,
            invoices.usdc_mint,
            invoices.paid_at,
            COALESCE((
                SELECT SUM(payments.amount_usdc)
                FROM payments
                WHERE payments.invoice_id = invoices.id
                  AND payments.status = 'confirmed'
            ), 0::numeric) AS paid_amount_usdc,
            (
                SELECT payments.tx_signature
                FROM payments
                WHERE payments.invoice_id = invoices.id
                  AND payments.status = 'confirmed'
                ORDER BY
                    COALESCE(payments.finalized_at, payments.created_at) DESC,
                    payments.created_at DESC
                LIMIT 1
            ) AS latest_payment_tx_signature,
            invoices.created_at
        FROM invoices
        ORDER BY invoices.created_at DESC
        "#,
    )
    .fetch_all(pool)
    .await?;

    Ok(invoices)
}

pub async fn get(pool: &PgPool, invoice_id: Uuid) -> AppResult<Invoice> {
    let invoice = sqlx::query_as::<_, Invoice>(
        r#"
        SELECT
            invoices.id,
            invoices.user_id,
            invoices.reference_pubkey,
            invoices.subtotal_usdc,
            invoices.platform_fee_usdc,
            invoices.platform_fee_bps,
            invoices.amount_usdc,
            invoices.description,
            invoices.client_email,
            invoices.status,
            invoices.wallet_pubkey,
            invoices.usdc_ata,
            invoices.usdc_mint,
            invoices.paid_at,
            COALESCE((
                SELECT SUM(payments.amount_usdc)
                FROM payments
                WHERE payments.invoice_id = invoices.id
                  AND payments.status = 'confirmed'
            ), 0::numeric) AS paid_amount_usdc,
            (
                SELECT payments.tx_signature
                FROM payments
                WHERE payments.invoice_id = invoices.id
                  AND payments.status = 'confirmed'
                ORDER BY
                    COALESCE(payments.finalized_at, payments.created_at) DESC,
                    payments.created_at DESC
                LIMIT 1
            ) AS latest_payment_tx_signature,
            invoices.created_at
        FROM invoices
        WHERE invoices.id = $1
        "#,
    )
    .bind(invoice_id)
    .fetch_one(pool)
    .await?;

    Ok(invoice)
}

pub async fn list_for_user(pool: &PgPool, user_id: Uuid) -> AppResult<Vec<Invoice>> {
    let invoices = sqlx::query_as::<_, Invoice>(
        r#"
        SELECT
            invoices.id,
            invoices.user_id,
            invoices.reference_pubkey,
            invoices.subtotal_usdc,
            invoices.platform_fee_usdc,
            invoices.platform_fee_bps,
            invoices.amount_usdc,
            invoices.description,
            invoices.client_email,
            invoices.status,
            invoices.wallet_pubkey,
            invoices.usdc_ata,
            invoices.usdc_mint,
            invoices.paid_at,
            COALESCE((
                SELECT SUM(payments.amount_usdc)
                FROM payments
                WHERE payments.invoice_id = invoices.id
                  AND payments.status = 'confirmed'
            ), 0::numeric) AS paid_amount_usdc,
            (
                SELECT payments.tx_signature
                FROM payments
                WHERE payments.invoice_id = invoices.id
                  AND payments.status = 'confirmed'
                ORDER BY
                    COALESCE(payments.finalized_at, payments.created_at) DESC,
                    payments.created_at DESC
                LIMIT 1
            ) AS latest_payment_tx_signature,
            invoices.created_at
        FROM invoices
        WHERE invoices.user_id = $1
        ORDER BY invoices.created_at DESC
        "#,
    )
    .bind(user_id)
    .fetch_all(pool)
    .await?;

    Ok(invoices)
}

pub async fn find_pending_match_by_reference(
    pool: &PgPool,
    references: &[String],
) -> AppResult<Option<InvoiceMatch>> {
    if references.is_empty() {
        return Ok(None);
    }

    let invoice = sqlx::query_as::<_, InvoiceMatch>(
        r#"
        SELECT id
        FROM invoices
        WHERE status = 'pending'
          AND reference_pubkey IS NOT NULL
          AND reference_pubkey = ANY($1)
        ORDER BY created_at ASC
        LIMIT 1
        "#,
    )
    .bind(references)
    .fetch_optional(pool)
    .await?;

    Ok(invoice)
}

pub async fn find_pending_match(
    pool: &PgPool,
    usdc_ata: &str,
    usdc_mint: &str,
    received_amount: Decimal,
    window_start: DateTime<Utc>,
    received_at: DateTime<Utc>,
) -> AppResult<Option<InvoiceMatch>> {
    let invoice = sqlx::query_as::<_, InvoiceMatch>(
        r#"
        SELECT id
        FROM invoices
        WHERE status = 'pending'
          AND usdc_ata = $1
          AND usdc_mint = $2
          AND amount_usdc <= $3
          AND created_at >= $4
          AND created_at <= $5
        ORDER BY
          CASE WHEN amount_usdc = $3 THEN 0 ELSE 1 END,
          amount_usdc DESC,
          created_at ASC
        LIMIT 1
        "#,
    )
    .bind(usdc_ata)
    .bind(usdc_mint)
    .bind(received_amount)
    .bind(window_start)
    .bind(received_at)
    .fetch_optional(pool)
    .await?;

    Ok(invoice)
}

pub async fn backfill_missing_references(pool: &PgPool) -> AppResult<u64> {
    let missing = sqlx::query_scalar::<_, Uuid>(
        r#"
        SELECT id
        FROM invoices
        WHERE reference_pubkey IS NULL
        "#,
    )
    .fetch_all(pool)
    .await?;

    for invoice_id in &missing {
        sqlx::query(
            r#"
            UPDATE invoices
            SET reference_pubkey = $2
            WHERE id = $1
            "#,
        )
        .bind(invoice_id)
        .bind(invoice_reference_pubkey(*invoice_id))
        .execute(pool)
        .await?;
    }

    Ok(missing.len() as u64)
}

pub fn invoice_reference_pubkey(invoice_id: Uuid) -> String {
    let digest = hash(invoice_id.as_bytes());
    Pubkey::new_from_array(digest.to_bytes()).to_string()
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

fn calculate_platform_fee(subtotal: Decimal) -> Decimal {
    (subtotal * Decimal::new(i64::from(PLATFORM_FEE_BPS), 4)).round_dp(6)
}

fn clean_optional(value: Option<String>) -> Option<String> {
    value.and_then(|raw| {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    })
}

#[derive(sqlx::FromRow)]
pub struct InvoiceMatch {
    pub id: Uuid,
}
