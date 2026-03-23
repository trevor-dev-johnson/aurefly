use axum::{
    extract::State,
    http::StatusCode,
    routing::post,
    Json, Router,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    error::AppResult,
    models::payment::Payment,
    services::payments::{self, CreatePayment},
    state::AppState,
};

pub fn router() -> Router<AppState> {
    Router::new().route("/", post(create_payment).get(list_payments))
}

#[derive(Debug, Deserialize)]
struct CreatePaymentRequest {
    invoice_id: Uuid,
    amount_usdc: String,
    tx_signature: String,
    payer_wallet_address: Option<String>,
}

#[derive(Debug, Serialize)]
struct PaymentResponse {
    id: Uuid,
    invoice_id: Uuid,
    amount_usdc: String,
    status: String,
    tx_signature: String,
    payer_wallet_address: Option<String>,
    recipient_token_account: String,
    token_mint: String,
    finalized_at: Option<DateTime<Utc>>,
    slot: Option<i64>,
    created_at: DateTime<Utc>,
}

impl From<Payment> for PaymentResponse {
    fn from(payment: Payment) -> Self {
        Self {
            id: payment.id,
            invoice_id: payment.invoice_id,
            amount_usdc: payment.amount_usdc.normalize().to_string(),
            status: payment.status,
            tx_signature: payment.tx_signature,
            payer_wallet_address: payment.payer_wallet_address,
            recipient_token_account: payment.recipient_token_account,
            token_mint: payment.token_mint,
            finalized_at: payment.finalized_at,
            slot: payment.slot,
            created_at: payment.created_at,
        }
    }
}

async fn create_payment(
    State(state): State<AppState>,
    Json(payload): Json<CreatePaymentRequest>,
) -> AppResult<(StatusCode, Json<PaymentResponse>)> {
    let payment = payments::create(
        &state.pool,
        CreatePayment {
            invoice_id: payload.invoice_id,
            amount_usdc: payload.amount_usdc,
            tx_signature: payload.tx_signature,
            payer_wallet_address: payload.payer_wallet_address,
            finalized_at: None,
            slot: None,
        },
    )
    .await?;

    Ok((StatusCode::CREATED, Json(payment.payment.into())))
}

async fn list_payments(State(state): State<AppState>) -> AppResult<Json<Vec<PaymentResponse>>> {
    let payments = payments::list(&state.pool).await?;
    let response = payments.into_iter().map(PaymentResponse::from).collect();
    Ok(Json(response))
}
