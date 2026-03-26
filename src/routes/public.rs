use axum::{
    extract::{Path, Query, State},
    http::header::CONTENT_TYPE,
    response::IntoResponse,
    routing::get,
    Json, Router,
};
use qrcode::{render::svg, QrCode};
use serde::Deserialize;
use uuid::Uuid;

use crate::{
    error::{AppError, AppResult},
    routes::invoices::{
        build_payment_uri, observe_invoice_payment, require_reference_pubkey, InvoiceResponse,
    },
    services::invoices,
    state::AppState,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/invoices/{invoice_id}", get(get_public_invoice))
        .route("/invoices/{invoice_id}/qr.svg", get(invoice_qr))
}

#[derive(Debug, Deserialize, Default)]
struct GetInvoiceQuery {
    observe_payment: Option<bool>,
}

async fn get_public_invoice(
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
    )?))
}

async fn invoice_qr(
    State(state): State<AppState>,
    Path(invoice_id): Path<Uuid>,
) -> AppResult<impl IntoResponse> {
    let invoice = invoices::get(&state.pool, invoice_id).await?;
    let reference_pubkey = require_reference_pubkey(invoice.id, invoice.reference_pubkey.as_deref())?;
    let payment_uri = build_payment_uri(
        &invoice.wallet_pubkey,
        &invoice.amount_usdc.normalize().to_string(),
        &invoice.usdc_mint,
        reference_pubkey,
    );
    let svg = QrCode::new(payment_uri.as_bytes())
        .map_err(|error| AppError::Internal(anyhow::anyhow!("failed to generate QR code: {error}")))?
        .render::<svg::Color<'_>>()
        .min_dimensions(320, 320)
        .dark_color(svg::Color("#0f172a"))
        .light_color(svg::Color("#f8fafc"))
        .build();

    Ok(([(CONTENT_TYPE, "image/svg+xml; charset=utf-8")], svg))
}
