use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use sqlx::FromRow;
use uuid::Uuid;

#[derive(Debug, Clone, FromRow)]
pub struct Payment {
    pub id: Uuid,
    pub invoice_id: Uuid,
    pub amount_usdc: Decimal,
    pub status: String,
    pub tx_signature: String,
    pub payer_wallet_address: Option<String>,
    pub recipient_token_account: String,
    pub token_mint: String,
    pub finalized_at: Option<DateTime<Utc>>,
    pub slot: Option<i64>,
    pub created_at: DateTime<Utc>,
}
