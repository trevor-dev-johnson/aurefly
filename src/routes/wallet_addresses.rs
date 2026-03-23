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
    models::wallet_address::WalletAddress,
    services::wallet_addresses::{self, CreateWalletAddress},
    state::AppState,
};

pub fn router() -> Router<AppState> {
    Router::new().route("/", post(create_wallet_address).get(list_wallet_addresses))
}

#[derive(Debug, Deserialize)]
struct CreateWalletAddressRequest {
    user_id: Uuid,
    wallet_pubkey: String,
    label: Option<String>,
}

#[derive(Debug, Serialize)]
struct WalletAddressResponse {
    id: Uuid,
    user_id: Uuid,
    wallet_pubkey: String,
    usdc_ata: String,
    usdc_mint: String,
    label: Option<String>,
    is_active: bool,
    created_at: DateTime<Utc>,
}

impl From<WalletAddress> for WalletAddressResponse {
    fn from(wallet_address: WalletAddress) -> Self {
        Self {
            id: wallet_address.id,
            user_id: wallet_address.user_id,
            wallet_pubkey: wallet_address.wallet_pubkey,
            usdc_ata: wallet_address.usdc_ata,
            usdc_mint: wallet_address.usdc_mint,
            label: wallet_address.label,
            is_active: wallet_address.is_active,
            created_at: wallet_address.created_at,
        }
    }
}

async fn create_wallet_address(
    State(state): State<AppState>,
    Json(payload): Json<CreateWalletAddressRequest>,
) -> AppResult<(StatusCode, Json<WalletAddressResponse>)> {
    let wallet_address = wallet_addresses::create(
        &state.pool,
        CreateWalletAddress {
            user_id: payload.user_id,
            wallet_pubkey: payload.wallet_pubkey,
            label: payload.label,
        },
    )
    .await?;

    Ok((StatusCode::CREATED, Json(wallet_address.into())))
}

async fn list_wallet_addresses(
    State(state): State<AppState>,
) -> AppResult<Json<Vec<WalletAddressResponse>>> {
    let wallet_addresses = wallet_addresses::list(&state.pool).await?;
    let response = wallet_addresses
        .into_iter()
        .map(WalletAddressResponse::from)
        .collect();

    Ok(Json(response))
}
