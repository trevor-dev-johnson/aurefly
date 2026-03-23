use axum::{
    extract::State,
    http::{HeaderMap, StatusCode},
    routing::get,
    Json, Router,
};
use serde::Deserialize;

use crate::{
    auth::require_user,
    error::AppResult,
    routes::invoices::InvoiceResponse,
    services::invoices::{self, CreateInvoice},
    state::AppState,
};

pub fn router() -> Router<AppState> {
    Router::new().route("/invoices", get(list_invoices).post(create_invoice))
}

#[derive(Debug, Deserialize)]
struct CreateInvoiceRequest {
    amount_usdc: String,
    description: Option<String>,
    client_email: Option<String>,
    payout_address: Option<String>,
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
            .collect(),
    ))
}

async fn create_invoice(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<CreateInvoiceRequest>,
) -> AppResult<(StatusCode, Json<InvoiceResponse>)> {
    let user = require_user(&headers, &state).await?;
    let invoice = invoices::create(
        &state.pool,
        &state.solana,
        &state.treasury,
        CreateInvoice {
            user_id: user.id,
            amount_usdc: payload.amount_usdc,
            description: payload.description,
            client_email: payload.client_email,
            payout_address: payload.payout_address,
        },
    )
    .await?;

    Ok((
        StatusCode::CREATED,
        Json(InvoiceResponse::from_private_invoice(invoice, None)),
    ))
}
