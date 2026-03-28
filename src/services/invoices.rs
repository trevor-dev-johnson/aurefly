use std::{str::FromStr, time::Duration};

use chrono::Utc;
use rust_decimal::Decimal;
use solana_sdk::{hash::hash, pubkey::Pubkey};
use sqlx::PgPool;
use uuid::Uuid;

use crate::{
    clients::solana::SolanaRpcClient,
    error::{AppError, AppResult},
    models::invoice::Invoice,
    solana::UsdcSettlement,
};

pub const PLATFORM_FEE_BPS: i16 = 0;

pub struct CreateInvoice {
    pub user_id: Uuid,
    pub client_request_id: Option<Uuid>,
    pub amount_usdc: String,
    pub description: Option<String>,
    pub client_email: Option<String>,
    pub payout_address: String,
}

pub async fn create(
    pool: &PgPool,
    solana: &SolanaRpcClient,
    input: CreateInvoice,
) -> AppResult<Invoice> {
    let invoice_id = Uuid::new_v4();
    let subtotal = parse_amount(&input.amount_usdc)?;
    let platform_fee = calculate_platform_fee(subtotal);
    let amount = subtotal;
    let reference_pubkey = invoice_reference_pubkey(invoice_id);
    let description = clean_optional(input.description);
    let client_email = clean_optional(input.client_email);
    let payout_address = input.payout_address.trim();
    if payout_address.is_empty() {
        return Err(AppError::Validation(
            "payout_address is required".to_string(),
        ));
    }

    let settlement = solana
        .resolve_usdc_settlement_target(payout_address)
        .await?;

    let invoice = sqlx::query_as::<_, Invoice>(
        r#"
        INSERT INTO invoices (
            id,
            user_id,
            client_request_id,
            reference_pubkey,
            requested_payout_address,
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
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, 'pending', $12, $13, $14, $15)
        ON CONFLICT (user_id, client_request_id)
        DO UPDATE SET client_request_id = EXCLUDED.client_request_id
        RETURNING
            id,
            user_id,
            reference_pubkey,
            requested_payout_address,
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
    .bind(input.client_request_id)
    .bind(&reference_pubkey)
    .bind(payout_address)
    .bind(subtotal)
    .bind(platform_fee)
    .bind(PLATFORM_FEE_BPS)
    .bind(amount)
    .bind(&description)
    .bind(&client_email)
    .bind(&settlement.usdc_ata)
    .bind(&settlement.wallet_pubkey)
    .bind(&settlement.usdc_ata)
    .bind(&settlement.usdc_mint)
    .fetch_one(pool)
    .await?;

    Ok(invoice)
}

pub async fn get(pool: &PgPool, invoice_id: Uuid) -> AppResult<Invoice> {
    let invoice = sqlx::query_as::<_, Invoice>(
        r#"
        SELECT
            invoices.id,
            invoices.user_id,
            invoices.reference_pubkey,
            invoices.requested_payout_address,
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
            invoices.requested_payout_address,
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

pub async fn cancel_for_user(pool: &PgPool, user_id: Uuid, invoice_id: Uuid) -> AppResult<Invoice> {
    let result = sqlx::query(
        r#"
        UPDATE invoices
        SET status = 'cancelled'
        WHERE id = $1
          AND user_id = $2
          AND status = 'pending'
        "#,
    )
    .bind(invoice_id)
    .bind(user_id)
    .execute(pool)
    .await?;

    if result.rows_affected() == 1 {
        return get(pool, invoice_id).await;
    }

    let existing_status = sqlx::query_scalar::<_, String>(
        r#"
        SELECT status
        FROM invoices
        WHERE id = $1
          AND user_id = $2
        "#,
    )
    .bind(invoice_id)
    .bind(user_id)
    .fetch_optional(pool)
    .await?;

    match existing_status.as_deref() {
        None => Err(AppError::NotFound),
        Some("cancelled") => Err(AppError::Validation(
            "invoice is already cancelled".to_string(),
        )),
        Some("paid") => Err(AppError::Validation(
            "paid invoices cannot be cancelled".to_string(),
        )),
        Some("expired") => Err(AppError::Validation(
            "expired invoices cannot be cancelled".to_string(),
        )),
        Some(_) => Err(AppError::Validation(
            "only pending invoices can be cancelled".to_string(),
        )),
    }
}

pub async fn find_reference_match_for_target(
    pool: &PgPool,
    usdc_ata: &str,
    usdc_mint: &str,
    references: &[String],
) -> AppResult<Option<ReferenceMatchCandidate>> {
    if references.is_empty() {
        return Ok(None);
    }

    let invoice = sqlx::query_as::<_, ReferenceMatchCandidate>(
        r#"
        SELECT id, reference_pubkey, amount_usdc, status
        FROM invoices
        WHERE usdc_ata = $1
          AND usdc_mint = $2
          AND reference_pubkey = ANY($3)
        ORDER BY created_at DESC
        LIMIT 1
        "#,
    )
    .bind(usdc_ata)
    .bind(usdc_mint)
    .bind(references)
    .fetch_optional(pool)
    .await?;

    Ok(invoice)
}

pub async fn find_reference_match_any(
    pool: &PgPool,
    references: &[String],
) -> AppResult<Option<ReferenceMatchCandidate>> {
    if references.is_empty() {
        return Ok(None);
    }

    let invoice = sqlx::query_as::<_, ReferenceMatchCandidate>(
        r#"
        SELECT id, reference_pubkey, amount_usdc, status
        FROM invoices
        WHERE reference_pubkey = ANY($1)
        ORDER BY created_at DESC
        LIMIT 1
        "#,
    )
    .bind(references)
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

pub async fn list_pending_settlement_targets(
    pool: &PgPool,
) -> AppResult<Vec<PendingSettlementTarget>> {
    let targets = sqlx::query_as::<_, PendingSettlementTarget>(
        r#"
        SELECT DISTINCT ON (usdc_ata, usdc_mint) usdc_ata, usdc_mint, wallet_pubkey
        FROM invoices
        WHERE status = 'pending'
        ORDER BY usdc_ata ASC, usdc_mint ASC, created_at DESC
        "#,
    )
    .fetch_all(pool)
    .await?;

    Ok(targets)
}

pub async fn expire_pending_older_than(pool: &PgPool, ttl: Duration) -> AppResult<u64> {
    let ttl_seconds = i64::try_from(ttl.as_secs()).map_err(|_| {
        AppError::Internal(anyhow::anyhow!("pending invoice TTL is too large to convert"))
    })?;
    let cutoff = Utc::now() - chrono::Duration::seconds(ttl_seconds);

    let result = sqlx::query(
        r#"
        UPDATE invoices
        SET status = 'expired'
        WHERE status = 'pending'
          AND created_at < $1
        "#,
    )
    .bind(cutoff)
    .execute(pool)
    .await?;

    Ok(result.rows_affected())
}

pub async fn expire_invalid_pending_destinations(pool: &PgPool) -> AppResult<u64> {
    let pending = sqlx::query_as::<_, PendingInvoiceDestination>(
        r#"
        SELECT id, wallet_pubkey, usdc_ata, usdc_mint
        FROM invoices
        WHERE status = 'pending'
        "#,
    )
    .fetch_all(pool)
    .await?;

    let invalid_ids = pending
        .into_iter()
        .filter_map(|invoice| match UsdcSettlement::from_wallet_pubkey(&invoice.wallet_pubkey) {
            Ok(settlement)
                if settlement.usdc_ata == invoice.usdc_ata
                    && settlement.usdc_mint == invoice.usdc_mint =>
            {
                None
            }
            Ok(_) | Err(_) => Some(invoice.id),
        })
        .collect::<Vec<_>>();

    if invalid_ids.is_empty() {
        return Ok(0);
    }

    let result = sqlx::query(
        r#"
        UPDATE invoices
        SET status = 'expired'
        WHERE status = 'pending'
          AND id = ANY($1)
        "#,
    )
    .bind(&invalid_ids)
    .execute(pool)
    .await?;

    Ok(result.rows_affected())
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

pub fn calculate_net_amount(amount: Decimal, platform_fee: Decimal) -> Decimal {
    let net = amount - platform_fee;
    if net.is_sign_negative() {
        Decimal::ZERO
    } else {
        net.round_dp(6)
    }
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

#[derive(Clone, sqlx::FromRow)]
pub struct ReferenceMatchCandidate {
    pub id: Uuid,
    pub reference_pubkey: String,
    pub amount_usdc: Decimal,
    pub status: String,
}

#[derive(sqlx::FromRow)]
pub struct PendingSettlementTarget {
    pub usdc_ata: String,
    pub usdc_mint: String,
    pub wallet_pubkey: String,
}

#[derive(sqlx::FromRow)]
struct PendingInvoiceDestination {
    id: Uuid,
    wallet_pubkey: String,
    usdc_ata: String,
    usdc_mint: String,
}
