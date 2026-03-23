use chrono::{DateTime, Utc};
use sqlx::FromRow;
use uuid::Uuid;

#[derive(Debug, Clone, FromRow)]
pub struct WalletAddress {
    pub id: Uuid,
    pub user_id: Uuid,
    pub wallet_pubkey: String,
    pub usdc_ata: String,
    pub usdc_mint: String,
    pub label: Option<String>,
    pub is_active: bool,
    pub created_at: DateTime<Utc>,
}
