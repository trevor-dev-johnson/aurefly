use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde_json::Value;
use sqlx::FromRow;
use uuid::Uuid;

#[derive(Debug, Clone, FromRow)]
pub struct UnmatchedPayment {
    pub id: Uuid,
    pub signature: String,
    pub destination_wallet: String,
    pub amount_usdc: Decimal,
    pub sender_wallet: Option<String>,
    pub reference_pubkey: Option<String>,
    pub seen_at: DateTime<Utc>,
    pub reason: String,
    pub status: String,
    pub linked_invoice_id: Option<Uuid>,
    pub notes: Option<String>,
    pub metadata: Value,
}

#[derive(Debug, Clone, FromRow)]
pub struct UnmatchedPaymentAuditEvent {
    pub id: Uuid,
    pub unmatched_payment_id: Uuid,
    pub actor_user_id: Option<Uuid>,
    pub actor_email: String,
    pub action: String,
    pub previous_status: Option<String>,
    pub next_status: Option<String>,
    pub linked_invoice_id: Option<Uuid>,
    pub note: Option<String>,
    pub metadata: Value,
    pub created_at: DateTime<Utc>,
}
