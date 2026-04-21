use axum::{
    extract::{Path, Query, State},
    http::HeaderMap,
    routing::{get, post},
    Json, Router,
};
use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use crate::{
    auth::require_admin,
    detector::DetectorRuntimeSnapshot,
    error::AppResult,
    models::{
        payment::Payment,
        unmatched_payment::{UnmatchedPayment, UnmatchedPaymentAuditEvent},
    },
    routes::invoices::InvoiceResponse,
    services::{invoices, payments, unmatched_payments},
    solana::MAINNET_USDC_MINT,
    state::AppState,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/detector", get(get_detector_status))
        .route("/unmatched-payments", get(list_unmatched_payments))
        .route(
            "/unmatched-payments/{unmatched_payment_id}",
            get(get_unmatched_payment),
        )
        .route(
            "/unmatched-payments/{unmatched_payment_id}/link",
            post(link_unmatched_payment),
        )
        .route(
            "/unmatched-payments/{unmatched_payment_id}/status",
            post(update_unmatched_payment_status),
        )
        .route(
            "/unmatched-payments/{unmatched_payment_id}/retry",
            post(retry_unmatched_payment_detection),
        )
}

async fn get_detector_status(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> AppResult<Json<DetectorRuntimeSnapshot>> {
    let _admin = require_admin(&headers, &state).await?;
    Ok(Json(state.detector_runtime.snapshot().await))
}

#[derive(Debug, Deserialize)]
struct ListUnmatchedPaymentsQuery {
    q: Option<String>,
    signature: Option<String>,
    invoice_id: Option<Uuid>,
    reference: Option<String>,
    wallet: Option<String>,
    amount_usdc: Option<String>,
    status: Option<String>,
    date_from: Option<DateTime<Utc>>,
    date_to: Option<DateTime<Utc>>,
    limit: Option<usize>,
}

#[derive(Debug, Deserialize)]
struct LinkUnmatchedPaymentRequest {
    invoice_id: Uuid,
    note: Option<String>,
}

#[derive(Debug, Deserialize)]
struct UpdateUnmatchedPaymentStatusRequest {
    status: String,
    note: Option<String>,
}

#[derive(Debug, Serialize)]
struct UnmatchedPaymentSummaryResponse {
    id: Uuid,
    signature: String,
    destination_wallet: String,
    amount_usdc: String,
    sender_wallet: Option<String>,
    reference_pubkey: Option<String>,
    seen_at: DateTime<Utc>,
    reason: String,
    status: String,
    linked_invoice_id: Option<Uuid>,
    notes: Option<String>,
}

#[derive(Debug, Serialize)]
struct AuditEventResponse {
    id: Uuid,
    action: String,
    actor_email: String,
    previous_status: Option<String>,
    next_status: Option<String>,
    linked_invoice_id: Option<Uuid>,
    note: Option<String>,
    metadata: Value,
    created_at: DateTime<Utc>,
}

#[derive(Debug, Serialize)]
struct PaymentSummaryResponse {
    invoice_id: Uuid,
    tx_signature: String,
    amount_usdc: String,
    payer_wallet_address: Option<String>,
    recipient_token_account: String,
    token_mint: String,
    finalized_at: Option<DateTime<Utc>>,
    created_at: DateTime<Utc>,
}

#[derive(Debug, Serialize)]
struct ChainSnapshotResponse {
    amount_usdc: String,
    source_owner: Option<String>,
    finalized_at: Option<DateTime<Utc>>,
    account_keys: Vec<String>,
    lookup_error: Option<String>,
}

#[derive(Debug, Serialize)]
struct UnmatchedPaymentDetailResponse {
    payment: UnmatchedPaymentSummaryResponse,
    linked_invoice: Option<InvoiceResponse>,
    existing_payment: Option<PaymentSummaryResponse>,
    audit_events: Vec<AuditEventResponse>,
    metadata: Value,
    chain_snapshot: Option<ChainSnapshotResponse>,
}

impl UnmatchedPaymentSummaryResponse {
    fn from_model(payment: UnmatchedPayment) -> Self {
        Self {
            id: payment.id,
            signature: payment.signature,
            destination_wallet: payment.destination_wallet,
            amount_usdc: normalize_decimal(payment.amount_usdc),
            sender_wallet: payment.sender_wallet,
            reference_pubkey: payment.reference_pubkey,
            seen_at: payment.seen_at,
            reason: payment.reason,
            status: payment.status,
            linked_invoice_id: payment.linked_invoice_id,
            notes: payment.notes,
        }
    }
}

async fn list_unmatched_payments(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<ListUnmatchedPaymentsQuery>,
) -> AppResult<Json<Vec<UnmatchedPaymentSummaryResponse>>> {
    let _admin = require_admin(&headers, &state).await?;
    let payments = unmatched_payments::list(
        &state.pool,
        unmatched_payments::UnmatchedPaymentFilters {
            q: query.q,
            signature: query.signature,
            invoice_id: query.invoice_id,
            reference_pubkey: query.reference,
            wallet: query.wallet,
            amount_usdc: query.amount_usdc,
            status: query.status,
            date_from: query.date_from,
            date_to: query.date_to,
            limit: query.limit,
        },
    )
    .await?;

    Ok(Json(
        payments
            .into_iter()
            .map(UnmatchedPaymentSummaryResponse::from_model)
            .collect(),
    ))
}

async fn get_unmatched_payment(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(unmatched_payment_id): Path<Uuid>,
) -> AppResult<Json<UnmatchedPaymentDetailResponse>> {
    let _admin = require_admin(&headers, &state).await?;
    Ok(Json(
        build_unmatched_payment_detail(&state, unmatched_payment_id).await?,
    ))
}

async fn link_unmatched_payment(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(unmatched_payment_id): Path<Uuid>,
    Json(payload): Json<LinkUnmatchedPaymentRequest>,
) -> AppResult<Json<UnmatchedPaymentDetailResponse>> {
    let admin = require_admin(&headers, &state).await?;
    unmatched_payments::link_to_invoice(
        &state.pool,
        unmatched_payment_id,
        unmatched_payments::ManualLinkUnmatchedPayment {
            invoice_id: payload.invoice_id,
            note: payload.note,
        },
        &admin,
    )
    .await?;

    Ok(Json(
        build_unmatched_payment_detail(&state, unmatched_payment_id).await?,
    ))
}

async fn update_unmatched_payment_status(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(unmatched_payment_id): Path<Uuid>,
    Json(payload): Json<UpdateUnmatchedPaymentStatusRequest>,
) -> AppResult<Json<UnmatchedPaymentDetailResponse>> {
    let admin = require_admin(&headers, &state).await?;
    unmatched_payments::update_status(
        &state.pool,
        unmatched_payment_id,
        unmatched_payments::UpdateUnmatchedPaymentStatus {
            status: payload.status,
            note: payload.note,
        },
        &admin,
    )
    .await?;

    Ok(Json(
        build_unmatched_payment_detail(&state, unmatched_payment_id).await?,
    ))
}

async fn retry_unmatched_payment_detection(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(unmatched_payment_id): Path<Uuid>,
) -> AppResult<Json<UnmatchedPaymentDetailResponse>> {
    let admin = require_admin(&headers, &state).await?;
    unmatched_payments::retry_detection(&state.pool, unmatched_payment_id, &admin).await?;

    Ok(Json(
        build_unmatched_payment_detail(&state, unmatched_payment_id).await?,
    ))
}

impl AuditEventResponse {
    fn from_model(event: UnmatchedPaymentAuditEvent) -> Self {
        Self {
            id: event.id,
            action: event.action,
            actor_email: event.actor_email,
            previous_status: event.previous_status,
            next_status: event.next_status,
            linked_invoice_id: event.linked_invoice_id,
            note: event.note,
            metadata: event.metadata,
            created_at: event.created_at,
        }
    }
}

impl PaymentSummaryResponse {
    fn from_model(payment: Payment) -> Self {
        Self {
            invoice_id: payment.invoice_id,
            tx_signature: payment.tx_signature,
            amount_usdc: normalize_decimal(payment.amount_usdc),
            payer_wallet_address: payment.payer_wallet_address,
            recipient_token_account: payment.recipient_token_account,
            token_mint: payment.token_mint,
            finalized_at: payment.finalized_at,
            created_at: payment.created_at,
        }
    }
}

fn normalize_decimal(value: Decimal) -> String {
    value.normalize().to_string()
}

async fn build_unmatched_payment_detail(
    state: &AppState,
    unmatched_payment_id: Uuid,
) -> AppResult<UnmatchedPaymentDetailResponse> {
    let payment = unmatched_payments::get(&state.pool, unmatched_payment_id).await?;
    let audit_events =
        unmatched_payments::list_audit_events(&state.pool, unmatched_payment_id).await?;

    let linked_invoice = match payment.linked_invoice_id {
        Some(invoice_id) => {
            let invoice = invoices::get(&state.pool, invoice_id).await?;
            Some(InvoiceResponse::from_private_invoice(invoice, None)?)
        }
        None => None,
    };

    let existing_payment = payments::get_by_tx_signature(&state.pool, &payment.signature)
        .await?
        .map(PaymentSummaryResponse::from_model);

    let chain_snapshot = match state
        .solana
        .get_finalized_usdc_transfer_to_token_account(
            &payment.signature,
            &payment.destination_wallet,
            MAINNET_USDC_MINT,
        )
        .await
    {
        Ok(Some(transfer)) => Some(ChainSnapshotResponse {
            amount_usdc: normalize_decimal(transfer.amount_usdc),
            source_owner: transfer.source_owner,
            finalized_at: transfer.finalized_at,
            account_keys: transfer.account_keys,
            lookup_error: None,
        }),
        Ok(None) => None,
        Err(error) => Some(ChainSnapshotResponse {
            amount_usdc: normalize_decimal(payment.amount_usdc),
            source_owner: payment.sender_wallet.clone(),
            finalized_at: None,
            account_keys: Vec::new(),
            lookup_error: Some(error.to_string()),
        }),
    };

    Ok(UnmatchedPaymentDetailResponse {
        payment: UnmatchedPaymentSummaryResponse::from_model(payment.clone()),
        linked_invoice,
        existing_payment,
        audit_events: audit_events
            .into_iter()
            .map(AuditEventResponse::from_model)
            .collect(),
        metadata: payment.metadata,
        chain_snapshot,
    })
}
