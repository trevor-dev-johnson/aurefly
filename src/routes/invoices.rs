use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    error::AppResult,
    models::invoice::Invoice,
    services::invoices::{self, CreateInvoice},
    state::AppState,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", post(create_invoice).get(list_invoices))
        .route("/{invoice_id}", get(get_invoice))
}

#[derive(Debug, Deserialize)]
struct CreateInvoiceRequest {
    user_id: Uuid,
    amount_usdc: String,
    description: Option<String>,
    client_email: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
struct GetInvoiceQuery {
    observe_payment: Option<bool>,
}

#[derive(Debug, Serialize)]
pub(crate) struct InvoiceResponse {
    id: Uuid,
    user_id: Uuid,
    reference_pubkey: Option<String>,
    subtotal_usdc: String,
    platform_fee_usdc: String,
    platform_fee_bps: i16,
    amount_usdc: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    client_email: Option<String>,
    paid_amount_usdc: String,
    status: String,
    wallet_pubkey: String,
    usdc_ata: String,
    usdc_mint: String,
    payment_uri: String,
    payment_observed: bool,
    payment_observed_tx_signature: Option<String>,
    payment_observed_tx_url: Option<String>,
    latest_payment_tx_signature: Option<String>,
    latest_payment_tx_url: Option<String>,
    paid_at: Option<DateTime<Utc>>,
    created_at: DateTime<Utc>,
}

impl InvoiceResponse {
    pub(crate) fn from_public_invoice(
        invoice: Invoice,
        payment_observation: Option<PaymentObservation>,
    ) -> Self {
        Self::from_invoice(invoice, payment_observation, false)
    }

    pub(crate) fn from_private_invoice(
        invoice: Invoice,
        payment_observation: Option<PaymentObservation>,
    ) -> Self {
        Self::from_invoice(invoice, payment_observation, true)
    }

    fn from_invoice(
        invoice: Invoice,
        payment_observation: Option<PaymentObservation>,
        include_client_email: bool,
    ) -> Self {
        let reference_pubkey = invoice.reference_pubkey.clone();
        let subtotal_usdc = invoice.subtotal_usdc.normalize().to_string();
        let platform_fee_usdc = invoice.platform_fee_usdc.normalize().to_string();
        let amount_usdc = invoice.amount_usdc.normalize().to_string();
        let paid_amount_usdc = invoice.paid_amount_usdc.normalize().to_string();
        let payment_uri = build_payment_uri(
            &invoice.usdc_ata,
            &amount_usdc,
            &invoice.usdc_mint,
            reference_pubkey.as_deref(),
        );
        let latest_payment_tx_url = invoice
            .latest_payment_tx_signature
            .as_deref()
            .map(build_explorer_tx_url);
        let payment_observed_tx_signature =
            payment_observation.as_ref().map(|observation| observation.tx_signature.clone());
        let payment_observed_tx_url = payment_observation
            .as_ref()
            .map(|observation| build_explorer_tx_url(&observation.tx_signature));

        Self {
            id: invoice.id,
            user_id: invoice.user_id,
            reference_pubkey,
            subtotal_usdc,
            platform_fee_usdc,
            platform_fee_bps: invoice.platform_fee_bps,
            amount_usdc,
            description: invoice.description,
            client_email: if include_client_email {
                invoice.client_email
            } else {
                None
            },
            paid_amount_usdc,
            status: invoice.status,
            wallet_pubkey: invoice.wallet_pubkey,
            usdc_ata: invoice.usdc_ata,
            usdc_mint: invoice.usdc_mint,
            payment_uri,
            payment_observed: payment_observation.is_some(),
            payment_observed_tx_signature,
            payment_observed_tx_url,
            latest_payment_tx_signature: invoice.latest_payment_tx_signature,
            latest_payment_tx_url,
            paid_at: invoice.paid_at,
            created_at: invoice.created_at,
        }
    }
}

fn build_explorer_tx_url(signature: &str) -> String {
    format!("https://explorer.solana.com/tx/{signature}?cluster=mainnet-beta")
}

pub(crate) fn build_payment_uri(
    usdc_ata: &str,
    amount_usdc: &str,
    usdc_mint: &str,
    reference_pubkey: Option<&str>,
) -> String {
    let mut payment_uri = format!("solana:{usdc_ata}?amount={amount_usdc}&spl-token={usdc_mint}");

    if let Some(reference_pubkey) = reference_pubkey {
        payment_uri.push_str("&reference=");
        payment_uri.push_str(reference_pubkey);
    }

    payment_uri
}

async fn create_invoice(
    State(state): State<AppState>,
    Json(payload): Json<CreateInvoiceRequest>,
) -> AppResult<(StatusCode, Json<InvoiceResponse>)> {
    let invoice = invoices::create(
        &state.pool,
        &state.treasury,
        CreateInvoice {
            user_id: payload.user_id,
            amount_usdc: payload.amount_usdc,
            description: payload.description,
            client_email: payload.client_email,
        },
    )
    .await?;

    Ok((
        StatusCode::CREATED,
        Json(InvoiceResponse::from_public_invoice(invoice, None)),
    ))
}

async fn list_invoices(State(state): State<AppState>) -> AppResult<Json<Vec<InvoiceResponse>>> {
    let invoices = invoices::list(&state.pool).await?;
    let response = invoices
        .into_iter()
        .map(|invoice| InvoiceResponse::from_public_invoice(invoice, None))
        .collect();
    Ok(Json(response))
}

async fn get_invoice(
    State(state): State<AppState>,
    Path(invoice_id): Path<Uuid>,
    Query(query): Query<GetInvoiceQuery>,
) -> AppResult<Json<InvoiceResponse>> {
    let invoice = invoices::get(&state.pool, invoice_id).await?;
    let payment_observation = if query.observe_payment.unwrap_or(false) {
        observe_invoice_payment(&state, &invoice).await?
    } else {
        None
    };

    Ok(Json(InvoiceResponse::from_public_invoice(
        invoice,
        payment_observation,
    )))
}

pub(crate) struct PaymentObservation {
    tx_signature: String,
}

async fn observe_invoice_payment(
    state: &AppState,
    invoice: &Invoice,
) -> AppResult<Option<PaymentObservation>> {
    if invoice.status == "paid" {
        return Ok(None);
    }

    let Some(reference_pubkey) = invoice.reference_pubkey.as_deref() else {
        return Ok(None);
    };

    let signatures = state
        .solana
        .get_confirmed_signatures_for_address(&invoice.usdc_ata, 12, None)
        .await?;

    for signature in signatures {
        if signature.err.is_some() {
            continue;
        }

        if let Some(block_time) = signature.block_time {
            if block_time + 5 < invoice.created_at.timestamp() {
                continue;
            }
        }

        let Some(transfer) = state
            .solana
            .get_confirmed_usdc_transfer_to_token_account(
                &signature.signature,
                &invoice.usdc_ata,
                &invoice.usdc_mint,
            )
            .await?
        else {
            continue;
        };

        if transfer
            .account_keys
            .iter()
            .any(|account_key| account_key == reference_pubkey)
        {
            return Ok(Some(PaymentObservation {
                tx_signature: signature.signature,
            }));
        }
    }

    Ok(None)
}
