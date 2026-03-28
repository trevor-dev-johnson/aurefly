use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use sqlx::FromRow;
use uuid::Uuid;

#[derive(Debug, Clone, FromRow)]
pub struct Invoice {
    pub id: Uuid,
    pub user_id: Uuid,
    pub reference_pubkey: Option<String>,
    pub requested_payout_address: String,
    pub subtotal_usdc: Decimal,
    pub platform_fee_usdc: Decimal,
    pub platform_fee_bps: i16,
    pub amount_usdc: Decimal,
    pub description: Option<String>,
    pub client_email: Option<String>,
    pub status: String,
    pub wallet_pubkey: String,
    pub usdc_ata: String,
    pub usdc_mint: String,
    pub paid_at: Option<DateTime<Utc>>,
    pub paid_amount_usdc: Decimal,
    pub latest_payment_tx_signature: Option<String>,
    pub created_at: DateTime<Utc>,
}
