use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Deserializer};
use solana_sdk::pubkey::Pubkey;
use std::str::FromStr;
use uuid::Uuid;

use crate::{
    auth::require_user,
    error::{AppError, AppResult},
    routes::invoices::InvoiceResponse,
    services::invoices::{self, CreateInvoice},
    state::AppState,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/invoices", get(list_invoices).post(create_invoice))
        .route("/invoices/{invoice_id}/cancel", post(cancel_invoice))
}

#[derive(Debug, Deserialize)]
struct CreateInvoiceRequest {
    amount_usdc: String,
    client_request_id: Option<String>,
    description: Option<String>,
    client_email: Option<String>,
    #[serde(default, deserialize_with = "deserialize_nullable_string")]
    payout_address: String,
}

async fn list_invoices(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> AppResult<Json<Vec<InvoiceResponse>>> {
    let user = require_user(&headers, &state).await?;
    let invoices = invoices::list_for_user(&state.pool, user.id).await?;
    Ok(Json(
        invoices
            .into_iter()
            .map(|invoice| InvoiceResponse::from_private_invoice(invoice, None))
            .collect::<AppResult<Vec<_>>>()?,
    ))
}

async fn create_invoice(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<CreateInvoiceRequest>,
) -> AppResult<(StatusCode, Json<InvoiceResponse>)> {
    let user = require_user(&headers, &state).await?;
    let payout_address = payload.payout_address.trim();
    if payout_address.is_empty() {
        return Err(AppError::Validation(
            "payout_address is required".to_string(),
        ));
    }

    Pubkey::from_str(payout_address)
        .map_err(|_| AppError::Validation("invalid payout_address".to_string()))?;
    let client_request_id = payload
        .client_request_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| {
            Uuid::parse_str(value)
                .map_err(|_| AppError::Validation("invalid client_request_id".to_string()))
        })
        .transpose()?;

    let invoice = invoices::create(
        &state.pool,
        &state.solana,
        CreateInvoice {
            user_id: user.id,
            client_request_id,
            amount_usdc: payload.amount_usdc,
            description: payload.description,
            client_email: payload.client_email,
            payout_address: payout_address.to_string(),
        },
    )
    .await?;

    Ok((
        StatusCode::CREATED,
        Json(InvoiceResponse::from_private_invoice(invoice, None)?),
    ))
}

async fn cancel_invoice(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(invoice_id): Path<Uuid>,
) -> AppResult<Json<InvoiceResponse>> {
    let user = require_user(&headers, &state).await?;
    let invoice = invoices::cancel_for_user(&state.pool, user.id, invoice_id).await?;

    Ok(Json(InvoiceResponse::from_private_invoice(invoice, None)?))
}

fn deserialize_nullable_string<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: Deserializer<'de>,
{
    Ok(Option::<String>::deserialize(deserializer)?.unwrap_or_default())
}
