use chrono::{DateTime, Utc};
use serde::Serialize;
use uuid::Uuid;

use crate::{
    error::{AppError, AppResult},
    models::invoice::Invoice,
    services::invoices,
};

#[derive(Debug, Serialize)]
pub(crate) struct InvoiceResponse {
    id: Uuid,
    user_id: Uuid,
    reference_pubkey: Option<String>,
    subtotal_usdc: String,
    platform_fee_usdc: String,
    platform_fee_bps: i16,
    amount_usdc: String,
    net_amount_usdc: String,
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

pub(crate) struct PaymentObservation {
    pub(crate) tx_signature: String,
}

impl InvoiceResponse {
    pub(crate) fn from_public_invoice(
        invoice: Invoice,
        payment_observation: Option<PaymentObservation>,
    ) -> AppResult<Self> {
        Self::from_invoice(invoice, payment_observation, false)
    }

    pub(crate) fn from_private_invoice(
        invoice: Invoice,
        payment_observation: Option<PaymentObservation>,
    ) -> AppResult<Self> {
        Self::from_invoice(invoice, payment_observation, true)
    }

    fn from_invoice(
        invoice: Invoice,
        payment_observation: Option<PaymentObservation>,
        include_client_email: bool,
    ) -> AppResult<Self> {
        let reference_pubkey =
            require_reference_pubkey(invoice.id, invoice.reference_pubkey.as_deref())?;
        let subtotal_usdc = invoice.subtotal_usdc.normalize().to_string();
        let platform_fee_usdc = invoice.platform_fee_usdc.normalize().to_string();
        let amount_usdc = invoice.amount_usdc.normalize().to_string();
        let net_amount_usdc = invoices::calculate_net_amount(invoice.amount_usdc, invoice.platform_fee_usdc)
            .normalize()
            .to_string();
        let paid_amount_usdc = invoice.paid_amount_usdc.normalize().to_string();
        let payment_uri = build_payment_uri(
            &invoice.wallet_pubkey,
            &amount_usdc,
            &invoice.usdc_mint,
            reference_pubkey,
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

        Ok(Self {
            id: invoice.id,
            user_id: invoice.user_id,
            reference_pubkey: Some(reference_pubkey.to_string()),
            subtotal_usdc,
            platform_fee_usdc,
            platform_fee_bps: invoice.platform_fee_bps,
            amount_usdc,
            net_amount_usdc,
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
        })
    }
}

fn build_explorer_tx_url(signature: &str) -> String {
    format!("https://explorer.solana.com/tx/{signature}?cluster=mainnet-beta")
}

pub(crate) fn build_payment_uri(
    wallet_pubkey: &str,
    amount_usdc: &str,
    usdc_mint: &str,
    reference_pubkey: &str,
) -> String {
    format!(
        "solana:{wallet_pubkey}?amount={amount_usdc}&spl-token={usdc_mint}&reference={reference_pubkey}"
    )
}

pub(crate) fn require_reference_pubkey<'a>(
    invoice_id: Uuid,
    reference_pubkey: Option<&'a str>,
) -> AppResult<&'a str> {
    match reference_pubkey.map(str::trim) {
        Some(value) if !value.is_empty() => Ok(value),
        _ => Err(AppError::Internal(anyhow::anyhow!(
            "invoice {invoice_id} is missing required Solana Pay reference"
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::build_payment_uri;

    #[test]
    fn payment_uri_uses_wallet_recipient_for_spl_transfers() {
        let wallet_pubkey = "GRLaUZb5s9DEsANDgqpUrzfeyCPW4MVtPQgUzDHHa9mR";
        let uri = build_payment_uri(
            wallet_pubkey,
            "0.5",
            "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v",
            "AVivqD14MroUWfSBQwV9V8zkh1n5Wpb1YrBkp4bj3a9o",
        );

        assert!(uri.starts_with(&format!("solana:{wallet_pubkey}?")));
    }
}
