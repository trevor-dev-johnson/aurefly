use std::str::FromStr;

use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde_json::{json, Value};
use sqlx::{PgPool, Postgres, QueryBuilder, Transaction};
use uuid::Uuid;

use crate::{
    error::{AppError, AppResult},
    models::{
        payment::Payment,
        unmatched_payment::{UnmatchedPayment, UnmatchedPaymentAuditEvent},
        user::User,
    },
    services::{invoices, payments},
    solana::MAINNET_USDC_MINT,
};

pub const STATUS_PENDING: &str = "pending";
pub const STATUS_REVIEWED: &str = "reviewed";
pub const STATUS_RESOLVED: &str = "resolved";
pub const STATUS_IGNORED: &str = "ignored";
pub const STATUS_REFUNDED_MANUALLY: &str = "refunded_manually";
pub const STATUS_NEEDS_INVESTIGATION: &str = "needs_investigation";

const INVOICE_STATUS_PENDING: &str = "pending";
const INVOICE_STATUS_EXPIRED: &str = "expired";
const INVOICE_STATUS_CANCELLED: &str = "cancelled";

pub struct CreateUnmatchedPayment {
    pub signature: String,
    pub destination_wallet: String,
    pub amount_usdc: String,
    pub sender_wallet: Option<String>,
    pub reference_pubkey: Option<String>,
    pub reason: String,
    pub linked_invoice_id: Option<Uuid>,
    pub metadata: Option<Value>,
}

#[derive(Default)]
pub struct UnmatchedPaymentFilters {
    pub q: Option<String>,
    pub signature: Option<String>,
    pub invoice_id: Option<Uuid>,
    pub reference_pubkey: Option<String>,
    pub wallet: Option<String>,
    pub amount_usdc: Option<String>,
    pub status: Option<String>,
    pub date_from: Option<DateTime<Utc>>,
    pub date_to: Option<DateTime<Utc>>,
    pub limit: Option<usize>,
}

pub struct UpdateUnmatchedPaymentStatus {
    pub status: String,
    pub note: Option<String>,
}

pub struct ManualLinkUnmatchedPayment {
    pub invoice_id: Uuid,
    pub note: Option<String>,
}

pub struct RetryDetectionResult {
    pub unmatched_payment: UnmatchedPayment,
    pub linked_invoice_id: Option<Uuid>,
    pub payment_inserted: bool,
}

pub async fn create(pool: &PgPool, input: CreateUnmatchedPayment) -> AppResult<bool> {
    let signature = input.signature.trim().to_string();
    if signature.is_empty() {
        return Err(AppError::Validation("signature is required".to_string()));
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
    let metadata = input.metadata.unwrap_or_else(|| json!({}));

    let mut transaction = pool.begin().await?;
    let result = sqlx::query(
        r#"
        INSERT INTO unmatched_payments (
            signature,
            destination_wallet,
            amount_usdc,
            sender_wallet,
            reference_pubkey,
            reason,
            status,
            linked_invoice_id,
            metadata
        )
        VALUES ($1, $2, $3, $4, $5, $6, 'pending', $7, $8)
        ON CONFLICT (signature) DO NOTHING
        "#,
    )
    .bind(&signature)
    .bind(&destination_wallet)
    .bind(amount)
    .bind(sender_wallet)
    .bind(reference_pubkey)
    .bind(&reason)
    .bind(input.linked_invoice_id)
    .bind(metadata.clone())
    .execute(&mut *transaction)
    .await?;

    if result.rows_affected() > 0 {
        let payment_id = sqlx::query_scalar::<_, Uuid>(
            r#"
            SELECT id
            FROM unmatched_payments
            WHERE signature = $1
            "#,
        )
        .bind(&signature)
        .fetch_one(&mut *transaction)
        .await?;

        append_audit_event_tx(
            &mut transaction,
            AuditEventInput {
                unmatched_payment_id: payment_id,
                actor_user_id: None,
                actor_email: "detector".to_string(),
                action: "detected_unmatched".to_string(),
                previous_status: None,
                next_status: Some(STATUS_PENDING.to_string()),
                linked_invoice_id: input.linked_invoice_id,
                note: None,
                metadata,
            },
        )
        .await?;
    }

    transaction.commit().await?;
    Ok(result.rows_affected() > 0)
}

pub async fn list(
    pool: &PgPool,
    filters: UnmatchedPaymentFilters,
) -> AppResult<Vec<UnmatchedPayment>> {
    let mut builder = QueryBuilder::<Postgres>::new(
        r#"
        SELECT
            id,
            signature,
            destination_wallet,
            amount_usdc,
            sender_wallet,
            reference_pubkey,
            seen_at,
            reason,
            status,
            linked_invoice_id,
            notes,
            metadata
        FROM unmatched_payments
        WHERE 1 = 1
        "#,
    );

    if let Some(status) = clean_optional(filters.status) {
        let status = normalize_status(&status)?;
        builder.push(" AND status = ").push_bind(status);
    }

    if let Some(signature) = clean_optional(filters.signature) {
        builder
            .push(" AND signature ILIKE ")
            .push_bind(format!("%{signature}%"));
    }

    if let Some(invoice_id) = filters.invoice_id {
        builder
            .push(" AND linked_invoice_id = ")
            .push_bind(invoice_id);
    }

    if let Some(reference_pubkey) = clean_optional(filters.reference_pubkey) {
        builder
            .push(" AND reference_pubkey ILIKE ")
            .push_bind(format!("%{reference_pubkey}%"));
    }

    if let Some(wallet) = clean_optional(filters.wallet) {
        let pattern = format!("%{wallet}%");
        builder
            .push(" AND (destination_wallet ILIKE ")
            .push_bind(pattern.clone())
            .push(" OR sender_wallet ILIKE ")
            .push_bind(pattern)
            .push(")");
    }

    if let Some(amount) = filters.amount_usdc {
        builder
            .push(" AND amount_usdc = ")
            .push_bind(parse_amount(&amount)?);
    }

    if let Some(date_from) = filters.date_from {
        builder.push(" AND seen_at >= ").push_bind(date_from);
    }

    if let Some(date_to) = filters.date_to {
        builder.push(" AND seen_at <= ").push_bind(date_to);
    }

    if let Some(q) = clean_optional(filters.q) {
        let pattern = format!("%{}%", q.to_lowercase());
        builder
            .push(" AND (LOWER(signature) LIKE ")
            .push_bind(pattern.clone())
            .push(" OR LOWER(COALESCE(reference_pubkey, '')) LIKE ")
            .push_bind(pattern.clone())
            .push(" OR LOWER(destination_wallet) LIKE ")
            .push_bind(pattern.clone())
            .push(" OR LOWER(COALESCE(sender_wallet, '')) LIKE ")
            .push_bind(pattern.clone())
            .push(" OR LOWER(COALESCE(notes, '')) LIKE ")
            .push_bind(pattern.clone())
            .push(" OR CAST(amount_usdc AS TEXT) LIKE ")
            .push_bind(pattern.clone())
            .push(" OR CAST(COALESCE(linked_invoice_id, '00000000-0000-0000-0000-000000000000'::uuid) AS TEXT) LIKE ")
            .push_bind(pattern)
            .push(")");
    }

    let limit = filters.limit.unwrap_or(100).clamp(1, 250) as i64;
    builder
        .push(" ORDER BY seen_at DESC, id DESC LIMIT ")
        .push_bind(limit);

    let payments = builder
        .build_query_as::<UnmatchedPayment>()
        .fetch_all(pool)
        .await?;

    Ok(payments)
}

pub async fn get(pool: &PgPool, unmatched_payment_id: Uuid) -> AppResult<UnmatchedPayment> {
    let payment = sqlx::query_as::<_, UnmatchedPayment>(
        r#"
        SELECT
            id,
            signature,
            destination_wallet,
            amount_usdc,
            sender_wallet,
            reference_pubkey,
            seen_at,
            reason,
            status,
            linked_invoice_id,
            notes,
            metadata
        FROM unmatched_payments
        WHERE id = $1
        "#,
    )
    .bind(unmatched_payment_id)
    .fetch_one(pool)
    .await?;

    Ok(payment)
}

pub async fn list_audit_events(
    pool: &PgPool,
    unmatched_payment_id: Uuid,
) -> AppResult<Vec<UnmatchedPaymentAuditEvent>> {
    let events = sqlx::query_as::<_, UnmatchedPaymentAuditEvent>(
        r#"
        SELECT
            id,
            unmatched_payment_id,
            actor_user_id,
            actor_email,
            action,
            previous_status,
            next_status,
            linked_invoice_id,
            note,
            metadata,
            created_at
        FROM unmatched_payment_audit_events
        WHERE unmatched_payment_id = $1
        ORDER BY created_at DESC
        "#,
    )
    .bind(unmatched_payment_id)
    .fetch_all(pool)
    .await?;

    Ok(events)
}

pub async fn update_status(
    pool: &PgPool,
    unmatched_payment_id: Uuid,
    input: UpdateUnmatchedPaymentStatus,
    actor: &User,
) -> AppResult<UnmatchedPayment> {
    let next_status = normalize_status(&input.status)?.to_string();
    let note = input.note.and_then(clean_optional);
    let previous = get(pool, unmatched_payment_id).await?;

    let updated = sqlx::query_as::<_, UnmatchedPayment>(
        r#"
        UPDATE unmatched_payments
        SET
            status = $2,
            notes = COALESCE($3, notes)
        WHERE id = $1
        RETURNING
            id,
            signature,
            destination_wallet,
            amount_usdc,
            sender_wallet,
            reference_pubkey,
            seen_at,
            reason,
            status,
            linked_invoice_id,
            notes,
            metadata
        "#,
    )
    .bind(unmatched_payment_id)
    .bind(&next_status)
    .bind(&note)
    .fetch_one(pool)
    .await?;

    append_audit_event(
        pool,
        AuditEventInput {
            unmatched_payment_id,
            actor_user_id: Some(actor.id),
            actor_email: actor.email.clone(),
            action: "status_updated".to_string(),
            previous_status: Some(previous.status),
            next_status: Some(next_status),
            linked_invoice_id: updated.linked_invoice_id,
            note,
            metadata: json!({}),
        },
    )
    .await?;

    Ok(updated)
}

pub async fn link_to_invoice(
    pool: &PgPool,
    unmatched_payment_id: Uuid,
    input: ManualLinkUnmatchedPayment,
    actor: &User,
) -> AppResult<UnmatchedPayment> {
    let note = input.note.and_then(clean_optional);
    let mut transaction = pool.begin().await?;
    let unmatched = get_for_update(&mut transaction, unmatched_payment_id).await?;
    let invoice = get_invoice_payment_target(&mut transaction, input.invoice_id).await?;

    validate_manual_link(&unmatched, &invoice)?;
    let payment_inserted =
        create_or_resolve_payment_tx(&mut transaction, &unmatched, &invoice).await?;

    let updated = sqlx::query_as::<_, UnmatchedPayment>(
        r#"
        UPDATE unmatched_payments
        SET
            status = $2,
            linked_invoice_id = $3,
            notes = COALESCE($4, notes)
        WHERE id = $1
        RETURNING
            id,
            signature,
            destination_wallet,
            amount_usdc,
            sender_wallet,
            reference_pubkey,
            seen_at,
            reason,
            status,
            linked_invoice_id,
            notes,
            metadata
        "#,
    )
    .bind(unmatched_payment_id)
    .bind(STATUS_RESOLVED)
    .bind(input.invoice_id)
    .bind(&note)
    .fetch_one(&mut *transaction)
    .await?;

    append_audit_event_tx(
        &mut transaction,
        AuditEventInput {
            unmatched_payment_id,
            actor_user_id: Some(actor.id),
            actor_email: actor.email.clone(),
            action: "linked_to_invoice".to_string(),
            previous_status: Some(unmatched.status),
            next_status: Some(STATUS_RESOLVED.to_string()),
            linked_invoice_id: Some(input.invoice_id),
            note,
            metadata: json!({
                "tx_signature": unmatched.signature,
                "payment_inserted": payment_inserted,
            }),
        },
    )
    .await?;

    transaction.commit().await?;
    Ok(updated)
}

pub async fn retry_detection(
    pool: &PgPool,
    unmatched_payment_id: Uuid,
    actor: &User,
) -> AppResult<RetryDetectionResult> {
    let unmatched = get(pool, unmatched_payment_id).await?;

    if let Some(existing) = payments::get_by_tx_signature(pool, &unmatched.signature).await? {
        let updated = mark_resolved_from_existing_payment(
            pool,
            &unmatched,
            existing.invoice_id,
            actor,
            "retry_resolved_existing_payment",
            json!({
                "payment_inserted": false,
                "invoice_id": existing.invoice_id,
            }),
        )
        .await?;

        return Ok(RetryDetectionResult {
            linked_invoice_id: Some(existing.invoice_id),
            payment_inserted: false,
            unmatched_payment: updated,
        });
    }

    let Some(reference_pubkey) = unmatched.reference_pubkey.clone() else {
        append_audit_event(
            pool,
            AuditEventInput {
                unmatched_payment_id,
                actor_user_id: Some(actor.id),
                actor_email: actor.email.clone(),
                action: "retry_failed".to_string(),
                previous_status: Some(unmatched.status.clone()),
                next_status: Some(unmatched.status.clone()),
                linked_invoice_id: unmatched.linked_invoice_id,
                note: Some("retry detection failed: missing reference".to_string()),
                metadata: json!({
                    "reason": "missing_reference",
                }),
            },
        )
        .await?;

        return Err(AppError::Validation(
            "retry detection failed: missing reference".to_string(),
        ));
    };

    let target_match = invoices::find_reference_match_for_target(
        pool,
        &unmatched.destination_wallet,
        MAINNET_USDC_MINT,
        &[reference_pubkey.clone()],
    )
    .await?;

    let Some(candidate) = target_match else {
        append_audit_event(
            pool,
            AuditEventInput {
                unmatched_payment_id,
                actor_user_id: Some(actor.id),
                actor_email: actor.email.clone(),
                action: "retry_failed".to_string(),
                previous_status: Some(unmatched.status.clone()),
                next_status: Some(unmatched.status.clone()),
                linked_invoice_id: unmatched.linked_invoice_id,
                note: Some(
                    "retry detection failed: no invoice matched this reference and destination"
                        .to_string(),
                ),
                metadata: json!({
                    "reason": "no_invoice_match",
                    "reference_pubkey": reference_pubkey,
                }),
            },
        )
        .await?;

        return Err(AppError::Validation(
            "retry detection failed: no invoice matched this reference and destination"
                .to_string(),
        ));
    };

    if !invoice_status_allows_manual_resolution(&candidate.status) {
        append_audit_event(
            pool,
            AuditEventInput {
                unmatched_payment_id,
                actor_user_id: Some(actor.id),
                actor_email: actor.email.clone(),
                action: "retry_failed".to_string(),
                previous_status: Some(unmatched.status.clone()),
                next_status: Some(unmatched.status.clone()),
                linked_invoice_id: Some(candidate.id),
                note: Some(
                    "retry detection failed: matched invoice is already paid".to_string(),
                ),
                metadata: json!({
                    "reason": "invoice_not_manually_resolvable",
                    "invoice_id": candidate.id,
                    "invoice_status": candidate.status,
                }),
            },
        )
        .await?;

        return Err(AppError::Validation(
            "retry detection failed: matched invoice is already paid".to_string(),
        ));
    }

    if unmatched.amount_usdc < candidate.amount_usdc {
        append_audit_event(
            pool,
            AuditEventInput {
                unmatched_payment_id,
                actor_user_id: Some(actor.id),
                actor_email: actor.email.clone(),
                action: "retry_failed".to_string(),
                previous_status: Some(unmatched.status.clone()),
                next_status: Some(unmatched.status.clone()),
                linked_invoice_id: Some(candidate.id),
                note: Some(
                    "retry detection failed: payment amount is below invoice total".to_string(),
                ),
                metadata: json!({
                    "reason": "amount_below_invoice_total",
                    "invoice_id": candidate.id,
                    "amount_usdc": unmatched.amount_usdc.normalize().to_string(),
                    "invoice_amount_usdc": candidate.amount_usdc.normalize().to_string(),
                }),
            },
        )
        .await?;

        return Err(AppError::Validation(
            "retry detection failed: payment amount is below invoice total".to_string(),
        ));
    }

    let linked = link_to_invoice(
        pool,
        unmatched_payment_id,
        ManualLinkUnmatchedPayment {
            invoice_id: candidate.id,
            note: Some("Retry detection matched this payment by reference.".to_string()),
        },
        actor,
    )
    .await?;

    append_audit_event(
        pool,
        AuditEventInput {
            unmatched_payment_id,
            actor_user_id: Some(actor.id),
            actor_email: actor.email.clone(),
            action: "retry_succeeded".to_string(),
            previous_status: Some(unmatched.status),
            next_status: Some(linked.status.clone()),
            linked_invoice_id: linked.linked_invoice_id,
            note: Some("Retry detection resolved this payment.".to_string()),
            metadata: json!({
                "reference_pubkey": reference_pubkey,
                "invoice_id": candidate.id,
            }),
        },
    )
    .await?;

    Ok(RetryDetectionResult {
        unmatched_payment: linked.clone(),
        linked_invoice_id: linked.linked_invoice_id,
        payment_inserted: true,
    })
}

fn validate_manual_link(
    unmatched: &UnmatchedPayment,
    invoice: &InvoicePaymentTarget,
) -> AppResult<()> {
    if unmatched.destination_wallet != invoice.usdc_ata {
        return Err(AppError::Validation(
            "unmatched payment destination does not match the invoice settlement account"
                .to_string(),
        ));
    }

    if unmatched.amount_usdc < invoice.amount_usdc {
        return Err(AppError::Validation(
            "unmatched payment amount is below the invoice total".to_string(),
        ));
    }

    if invoice.usdc_mint != MAINNET_USDC_MINT {
        return Err(AppError::Validation(
            "invoice is not a mainnet USDC invoice".to_string(),
        ));
    }

    Ok(())
}

async fn create_or_resolve_payment_tx(
    transaction: &mut Transaction<'_, Postgres>,
    unmatched: &UnmatchedPayment,
    invoice: &InvoicePaymentTarget,
) -> AppResult<bool> {
    let existing = get_payment_by_tx_signature_tx(transaction, &unmatched.signature).await?;
    if let Some(existing) = existing {
        if existing.invoice_id != invoice.id {
            return Err(AppError::Validation(
                "tx_signature already exists for a different invoice".to_string(),
            ));
        }

        return Ok(false);
    }

    if !invoice_status_allows_manual_resolution(&invoice.status) {
        return Err(AppError::Validation(
            "only unpaid invoices can be linked unless the payment is already recorded for that invoice"
                .to_string(),
        ));
    }

    sqlx::query_as::<_, Payment>(
        r#"
        INSERT INTO payments (
            invoice_id,
            amount_usdc,
            status,
            tx_signature,
            payer_wallet_address,
            recipient_token_account,
            token_mint
        )
        VALUES ($1, $2, 'confirmed', $3, $4, $5, $6)
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
    .bind(invoice.id)
    .bind(unmatched.amount_usdc)
    .bind(&unmatched.signature)
    .bind(&unmatched.sender_wallet)
    .bind(&invoice.usdc_ata)
    .bind(&invoice.usdc_mint)
    .fetch_one(&mut **transaction)
    .await?;

    let confirmed_total = sqlx::query_scalar::<_, Decimal>(
        r#"
        SELECT COALESCE(SUM(amount_usdc), 0::numeric)
        FROM payments
        WHERE invoice_id = $1 AND status = 'confirmed'
        "#,
    )
    .bind(invoice.id)
    .fetch_one(&mut **transaction)
    .await?;

    if confirmed_total >= invoice.amount_usdc {
        sqlx::query(
            r#"
            UPDATE invoices
            SET status = 'paid', paid_at = COALESCE(paid_at, NOW())
            WHERE id = $1
            "#,
        )
        .bind(invoice.id)
        .execute(&mut **transaction)
        .await?;
    }

    Ok(true)
}

async fn mark_resolved_from_existing_payment(
    pool: &PgPool,
    unmatched: &UnmatchedPayment,
    invoice_id: Uuid,
    actor: &User,
    action: &str,
    metadata: Value,
) -> AppResult<UnmatchedPayment> {
    let updated = sqlx::query_as::<_, UnmatchedPayment>(
        r#"
        UPDATE unmatched_payments
        SET
            status = $2,
            linked_invoice_id = $3
        WHERE id = $1
        RETURNING
            id,
            signature,
            destination_wallet,
            amount_usdc,
            sender_wallet,
            reference_pubkey,
            seen_at,
            reason,
            status,
            linked_invoice_id,
            notes,
            metadata
        "#,
    )
    .bind(unmatched.id)
    .bind(STATUS_RESOLVED)
    .bind(invoice_id)
    .fetch_one(pool)
    .await?;

    append_audit_event(
        pool,
        AuditEventInput {
            unmatched_payment_id: unmatched.id,
            actor_user_id: Some(actor.id),
            actor_email: actor.email.clone(),
            action: action.to_string(),
            previous_status: Some(unmatched.status.clone()),
            next_status: Some(STATUS_RESOLVED.to_string()),
            linked_invoice_id: Some(invoice_id),
            note: Some("Matched to an already-recorded payment.".to_string()),
            metadata,
        },
    )
    .await?;

    Ok(updated)
}

async fn get_for_update(
    transaction: &mut Transaction<'_, Postgres>,
    unmatched_payment_id: Uuid,
) -> AppResult<UnmatchedPayment> {
    let payment = sqlx::query_as::<_, UnmatchedPayment>(
        r#"
        SELECT
            id,
            signature,
            destination_wallet,
            amount_usdc,
            sender_wallet,
            reference_pubkey,
            seen_at,
            reason,
            status,
            linked_invoice_id,
            notes,
            metadata
        FROM unmatched_payments
        WHERE id = $1
        FOR UPDATE
        "#,
    )
    .bind(unmatched_payment_id)
    .fetch_one(&mut **transaction)
    .await?;

    Ok(payment)
}

async fn get_invoice_payment_target(
    transaction: &mut Transaction<'_, Postgres>,
    invoice_id: Uuid,
) -> AppResult<InvoicePaymentTarget> {
    let invoice = sqlx::query_as::<_, InvoicePaymentTarget>(
        r#"
        SELECT id, amount_usdc, status, usdc_ata, usdc_mint
        FROM invoices
        WHERE id = $1
        FOR UPDATE
        "#,
    )
    .bind(invoice_id)
    .fetch_one(&mut **transaction)
    .await?;

    Ok(invoice)
}

async fn get_payment_by_tx_signature_tx(
    transaction: &mut Transaction<'_, Postgres>,
    tx_signature: &str,
) -> AppResult<Option<Payment>> {
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
    .fetch_optional(&mut **transaction)
    .await?;

    Ok(payment)
}

async fn append_audit_event(pool: &PgPool, input: AuditEventInput) -> AppResult<()> {
    let mut transaction = pool.begin().await?;
    append_audit_event_tx(&mut transaction, input).await?;
    transaction.commit().await?;
    Ok(())
}

async fn append_audit_event_tx(
    transaction: &mut Transaction<'_, Postgres>,
    input: AuditEventInput,
) -> AppResult<()> {
    sqlx::query(
        r#"
        INSERT INTO unmatched_payment_audit_events (
            unmatched_payment_id,
            actor_user_id,
            actor_email,
            action,
            previous_status,
            next_status,
            linked_invoice_id,
            note,
            metadata
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
        "#,
    )
    .bind(input.unmatched_payment_id)
    .bind(input.actor_user_id)
    .bind(input.actor_email)
    .bind(input.action)
    .bind(input.previous_status)
    .bind(input.next_status)
    .bind(input.linked_invoice_id)
    .bind(input.note)
    .bind(input.metadata)
    .execute(&mut **transaction)
    .await?;

    Ok(())
}

fn clean_optional<T>(value: T) -> Option<String>
where
    T: Into<Option<String>>,
{
    value.into().and_then(|value| {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    })
}

fn parse_amount(raw_amount: &str) -> AppResult<Decimal> {
    let amount = Decimal::from_str(raw_amount.trim()).map_err(|_| {
        AppError::Validation("amount_usdc must be a valid decimal string".to_string())
    })?;

    if amount <= Decimal::ZERO {
        return Err(AppError::Validation(
            "amount_usdc must be greater than zero".to_string(),
        ));
    }

    Ok(amount.round_dp(6))
}

fn normalize_status(value: &str) -> AppResult<&'static str> {
    match value.trim() {
        STATUS_PENDING => Ok(STATUS_PENDING),
        STATUS_REVIEWED => Ok(STATUS_REVIEWED),
        STATUS_RESOLVED => Ok(STATUS_RESOLVED),
        STATUS_IGNORED => Ok(STATUS_IGNORED),
        STATUS_REFUNDED_MANUALLY => Ok(STATUS_REFUNDED_MANUALLY),
        STATUS_NEEDS_INVESTIGATION => Ok(STATUS_NEEDS_INVESTIGATION),
        _ => Err(AppError::Validation(
            "invalid unmatched payment status".to_string(),
        )),
    }
}

fn invoice_status_allows_manual_resolution(status: &str) -> bool {
    matches!(
        status.trim(),
        INVOICE_STATUS_PENDING | INVOICE_STATUS_EXPIRED | INVOICE_STATUS_CANCELLED
    )
}

#[derive(sqlx::FromRow)]
struct InvoicePaymentTarget {
    id: Uuid,
    amount_usdc: Decimal,
    status: String,
    usdc_ata: String,
    usdc_mint: String,
}

struct AuditEventInput {
    unmatched_payment_id: Uuid,
    actor_user_id: Option<Uuid>,
    actor_email: String,
    action: String,
    previous_status: Option<String>,
    next_status: Option<String>,
    linked_invoice_id: Option<Uuid>,
    note: Option<String>,
    metadata: Value,
}

#[cfg(test)]
mod tests {
    use super::invoice_status_allows_manual_resolution;

    #[test]
    fn manual_resolution_allows_unpaid_invoice_statuses() {
        assert!(invoice_status_allows_manual_resolution("pending"));
        assert!(invoice_status_allows_manual_resolution("expired"));
        assert!(invoice_status_allows_manual_resolution("cancelled"));
    }

    #[test]
    fn manual_resolution_rejects_paid_invoice_status() {
        assert!(!invoice_status_allows_manual_resolution("paid"));
    }

    #[test]
    fn manual_resolution_rejects_unknown_invoice_status() {
        assert!(!invoice_status_allows_manual_resolution("processing"));
    }
}
