use std::{
    collections::{HashMap, HashSet},
    time::Duration,
};

use chrono::{DateTime, Utc};
use futures_util::{SinkExt, StreamExt};
use rust_decimal::Decimal;
use serde::Deserialize;
use sqlx::PgPool;
use tokio::time::Instant;
use tokio::task::JoinHandle;
use tokio_tungstenite::{connect_async, tungstenite::Message};
use uuid::Uuid;

use crate::{
    clients::solana::{SignatureInfo, SolanaRpcClient},
    error::{AppError, AppResult},
    solana::UsdcSettlement,
    services::{invoices, payments, unmatched_payments},
};

#[derive(Clone)]
pub struct PaymentDetectorConfig {
    pub poll_interval: Duration,
    pub signature_limit: usize,
}

#[derive(Default)]
struct DetectorCursor {
    last_seen_signature: Option<String>,
}

#[derive(Default)]
struct PollStats {
    pending_target_count: usize,
    new_signature_count: usize,
    processed_signature_count: usize,
}

const DETECTOR_HEARTBEAT_INTERVAL: Duration = Duration::from_secs(30);

pub async fn run(
    pool: PgPool,
    solana: SolanaRpcClient,
    config: PaymentDetectorConfig,
) {
    tracing::info!(
        signal_source = DetectionSource::Polling.as_str(),
        poll_interval_secs = config.poll_interval.as_secs(),
        signature_limit = config.signature_limit,
        "payment detector polling loop started"
    );
    tracing::info!(
        signal_source = DetectionSource::LogsSubscribe.as_str(),
        websocket_enabled = solana.websocket_url().is_some(),
        "payment detector websocket manager started"
    );

    let poll_pool = pool.clone();
    let poll_solana = solana.clone();
    let poll_config = config.clone();

    tokio::join!(
        run_poll_loop(poll_pool, poll_solana, poll_config),
        run_logs_manager_loop(pool, solana)
    );
}

async fn run_poll_loop(
    pool: PgPool,
    solana: SolanaRpcClient,
    config: PaymentDetectorConfig,
) {
    let mut cursors = HashMap::<String, DetectorCursor>::new();
    let mut consecutive_rate_limits = 0u32;
    let mut last_heartbeat = Instant::now() - DETECTOR_HEARTBEAT_INTERVAL;

    loop {
        match poll_once(&pool, &solana, &config, &mut cursors).await {
            Ok(stats) => {
                consecutive_rate_limits = 0;
                if last_heartbeat.elapsed() >= DETECTOR_HEARTBEAT_INTERVAL {
                    tracing::info!(
                        signal_source = DetectionSource::Polling.as_str(),
                        pending_targets = stats.pending_target_count,
                        tracked_cursors = cursors.len(),
                        new_signatures = stats.new_signature_count,
                        processed_signatures = stats.processed_signature_count,
                        poll_interval_secs = config.poll_interval.as_secs(),
                        "payment detector alive"
                    );
                    last_heartbeat = Instant::now();
                }
                tokio::time::sleep(config.poll_interval).await;
            }
            Err(error @ AppError::RateLimited { .. }) => {
                consecutive_rate_limits = consecutive_rate_limits.saturating_add(1);
                let retry_after = error.retry_after().unwrap_or(config.poll_interval);
                let cooldown =
                    rate_limit_cooldown(config.poll_interval, retry_after, consecutive_rate_limits);

                tracing::warn!(
                    error = %error,
                    consecutive_rate_limits,
                    cooldown_secs = cooldown.as_secs(),
                    "payment detector rate limited; backing off"
                );

                tokio::time::sleep(cooldown).await;
            }
            Err(error) => {
                consecutive_rate_limits = 0;
                tracing::error!(error = %error, "payment detector cycle failed");
                tokio::time::sleep(config.poll_interval).await;
            }
        }
    }
}

async fn run_logs_manager_loop(pool: PgPool, solana: SolanaRpcClient) {
    let Some(ws_url) = solana.websocket_url().map(str::to_string) else {
        tracing::warn!("payment detector websocket disabled: unable to derive websocket URL from RPC URL");
        return;
    };
    let mut subscriptions = HashMap::<String, JoinHandle<()>>::new();
    let mut last_heartbeat = Instant::now() - DETECTOR_HEARTBEAT_INTERVAL;

    loop {
        match invoices::list_pending_settlement_targets(&pool).await {
            Ok(targets) => {
                let targets = sanitize_pending_targets(targets);
                let active_targets = targets
                    .iter()
                    .map(|target| target.usdc_ata.clone())
                    .collect::<HashSet<_>>();

                let stale_targets = subscriptions
                    .keys()
                    .filter(|target| !active_targets.contains(*target))
                    .cloned()
                    .collect::<Vec<_>>();

                for stale_target in stale_targets {
                    if let Some(handle) = subscriptions.remove(&stale_target) {
                        handle.abort();
                        tracing::info!(
                            recipient_token_account = %stale_target,
                            signal_source = DetectionSource::LogsSubscribe.as_str(),
                            "payment detector stopped logs subscription for settled destination"
                        );
                    }
                }

                for target in targets {
                    if subscriptions.contains_key(&target.usdc_ata) {
                        continue;
                    }

                    let task_pool = pool.clone();
                    let task_solana = solana.clone();
                    let ws_url = ws_url.clone();
                    let recipient_token_account = target.usdc_ata.clone();
                    let token_mint = target.usdc_mint.clone();

                    let handle = tokio::spawn(async move {
                        run_logs_subscription_loop_for_target(
                            task_pool,
                            task_solana,
                            ws_url,
                            recipient_token_account,
                            token_mint,
                        )
                        .await;
                    });

                    subscriptions.insert(target.usdc_ata, handle);
                }

                if last_heartbeat.elapsed() >= DETECTOR_HEARTBEAT_INTERVAL {
                    tracing::info!(
                        signal_source = DetectionSource::LogsSubscribe.as_str(),
                        pending_targets = active_targets.len(),
                        active_subscriptions = subscriptions.len(),
                        "payment detector websocket manager alive"
                    );
                    last_heartbeat = Instant::now();
                }
            }
            Err(error) => {
                tracing::warn!(error = %error, "payment detector failed to refresh pending websocket targets");
            }
        }

        tokio::time::sleep(Duration::from_secs(10)).await;
    }
}

async fn run_logs_subscription_loop_for_target(
    pool: PgPool,
    solana: SolanaRpcClient,
    ws_url: String,
    recipient_token_account: String,
    token_mint: String,
) {
    let mut reconnect_backoff = Duration::from_secs(1);

    loop {
        match consume_logs_subscription(
            &pool,
            &solana,
            &ws_url,
            &recipient_token_account,
            &token_mint,
        )
        .await
        {
            Ok(()) => {
                reconnect_backoff = Duration::from_secs(1);
                tracing::warn!(
                    recipient_token_account = %recipient_token_account,
                    reconnect_delay_secs = reconnect_backoff.as_secs(),
                    signal_source = DetectionSource::LogsSubscribe.as_str(),
                    "payment detector logs subscription ended; reconnecting"
                );
            }
            Err(error) => {
                tracing::warn!(
                    error = %error,
                    recipient_token_account = %recipient_token_account,
                    reconnect_delay_secs = reconnect_backoff.as_secs(),
                    signal_source = DetectionSource::LogsSubscribe.as_str(),
                    "payment detector logs subscription failed; reconnecting"
                );
            }
        }

        tokio::time::sleep(reconnect_backoff).await;
        reconnect_backoff = websocket_backoff(reconnect_backoff);
    }
}

async fn poll_once(
    pool: &PgPool,
    solana: &SolanaRpcClient,
    config: &PaymentDetectorConfig,
    cursors: &mut HashMap<String, DetectorCursor>,
) -> AppResult<PollStats> {
    let targets = sanitize_pending_targets(invoices::list_pending_settlement_targets(pool).await?);
    let mut stats = PollStats {
        pending_target_count: targets.len(),
        ..PollStats::default()
    };
    let active_targets = targets
        .iter()
        .map(|target| target.usdc_ata.clone())
        .collect::<HashSet<_>>();
    cursors.retain(|target, _| active_targets.contains(target));

    for target in targets {
        let cursor = cursors.entry(target.usdc_ata.clone()).or_default();
        let mut signatures = solana
            .get_finalized_signatures_for_address(
                &target.usdc_ata,
                config.signature_limit,
                cursor.last_seen_signature.as_deref(),
            )
            .await?;

        if signatures.is_empty() {
            continue;
        }

        tracing::info!(
            recipient_token_account = %target.usdc_ata,
            token_mint = %target.usdc_mint,
            new_signatures = signatures.len(),
            signal_source = DetectionSource::Polling.as_str(),
            "payment detector found finalized signatures to inspect"
        );
        stats.new_signature_count += signatures.len();

        let newest_signature = signatures.first().map(|signature| signature.signature.clone());
        signatures.reverse();

        for signature in signatures {
            process_signature(
                pool,
                solana,
                &target.usdc_ata,
                &target.usdc_mint,
                &signature,
                DetectionSource::Polling,
            )
            .await?;
            stats.processed_signature_count += 1;
        }

        cursor.last_seen_signature = newest_signature;
    }

    Ok(stats)
}

async fn consume_logs_subscription(
    pool: &PgPool,
    solana: &SolanaRpcClient,
    ws_url: &str,
    recipient_token_account: &str,
    token_mint: &str,
) -> AppResult<()> {
    let (mut socket, _) = connect_async(ws_url).await.map_err(|error| {
        AppError::Internal(anyhow::anyhow!(
            "failed to connect Solana logs websocket: {error}"
        ))
    })?;

    let subscribe_request = serde_json::to_string(&serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "logsSubscribe",
        "params": [
            {
                "mentions": [recipient_token_account]
            },
            {
                "commitment": "finalized"
            }
        ]
    }))
    .map_err(|error| AppError::Internal(anyhow::Error::new(error)))?;

    socket
        .send(Message::Text(subscribe_request))
        .await
        .map_err(|error| {
            AppError::Internal(anyhow::anyhow!(
                "failed to subscribe to Solana logs websocket: {error}"
            ))
        })?;

    tracing::info!(
        recipient_token_account = %recipient_token_account,
        token_mint = %token_mint,
        signal_source = DetectionSource::LogsSubscribe.as_str(),
        "payment detector subscribed to finalized Solana logs"
    );

    while let Some(message) = socket.next().await {
        let message = message.map_err(|error| {
            AppError::Internal(anyhow::anyhow!(
                "failed to read Solana logs websocket message: {error}"
            ))
        })?;

        match message {
            Message::Text(payload) => {
                if let Some(ack) = parse_logs_subscribe_ack(&payload)? {
                    tracing::info!(
                        subscription_id = ack.subscription_id,
                        signal_source = DetectionSource::LogsSubscribe.as_str(),
                        "payment detector logs subscription is active"
                    );
                    continue;
                }

                let Some(notification) = parse_logs_notification(&payload)? else {
                    continue;
                };

                let signature = SignatureInfo {
                    signature: notification.value.signature,
                    err: notification.value.err,
                    block_time: None,
                    confirmation_status: Some("finalized".to_string()),
                    slot: notification.context.slot,
                };

                process_signature(
                    pool,
                    solana,
                    recipient_token_account,
                    token_mint,
                    &signature,
                    DetectionSource::LogsSubscribe,
                )
                .await?;
            }
            Message::Ping(payload) => {
                socket.send(Message::Pong(payload)).await.map_err(|error| {
                    AppError::Internal(anyhow::anyhow!(
                        "failed to respond to Solana logs websocket ping: {error}"
                    ))
                })?;
            }
            Message::Close(frame) => {
                tracing::warn!(
                    ?frame,
                    signal_source = DetectionSource::LogsSubscribe.as_str(),
                    "payment detector logs websocket closed"
                );
                return Ok(());
            }
            _ => {}
        }
    }

    Ok(())
}

async fn process_signature(
    pool: &PgPool,
    solana: &SolanaRpcClient,
    recipient_token_account: &str,
    token_mint: &str,
    signature: &SignatureInfo,
    detection_source: DetectionSource,
) -> AppResult<()> {
    if signature.confirmation_status.as_deref() != Some("finalized") {
        tracing::info!(
            tx_signature = %signature.signature,
            confirmation_status = ?signature.confirmation_status,
            signal_source = detection_source.as_str(),
            result = "ignored",
            reason = "not_finalized",
            "detector ignored transaction"
        );
        return Ok(());
    }

    if signature.err.is_some() {
        tracing::info!(
            tx_signature = %signature.signature,
            signal_source = detection_source.as_str(),
            result = "ignored",
            reason = "transaction_error",
            "detector ignored transaction"
        );
        return Ok(());
    }

    if payments::tx_signature_exists(pool, &signature.signature).await? {
        tracing::info!(
            tx_signature = %signature.signature,
            signal_source = detection_source.as_str(),
            result = "ignored",
            reason = "already_processed",
            "detector ignored transaction"
        );
        return Ok(());
    }

    let transfer = solana
        .get_finalized_usdc_transfer_to_token_account(
            &signature.signature,
            recipient_token_account,
            token_mint,
        )
        .await
        .map_err(|error| {
            tracing::warn!(
                error = %error,
                tx_signature = %signature.signature,
                recipient_token_account = %recipient_token_account,
                token_mint = %token_mint,
                signal_source = detection_source.as_str(),
                "payment detector failed while fetching finalized transaction details"
            );
            error
        })?;
    let Some(transfer) = transfer
    else {
        tracing::info!(
            tx_signature = %signature.signature,
            recipient_token_account = %recipient_token_account,
            token_mint = %token_mint,
            signal_source = detection_source.as_str(),
            result = "ignored",
            reason = "not_usdc_transfer_to_invoice_destination",
            "detector ignored transaction"
        );
        return Ok(());
    };

    let received_at = transfer
        .finalized_at
        .or_else(|| timestamp_to_utc(signature.block_time))
        .unwrap_or_else(Utc::now);
    let target_reference_match = invoices::find_reference_match_for_target(
        pool,
        recipient_token_account,
        token_mint,
        &transfer.account_keys,
    )
    .await
    .map_err(|error| {
        tracing::error!(
            error = %error,
            tx_signature = %signature.signature,
            recipient_token_account = %recipient_token_account,
            token_mint = %token_mint,
            signal_source = detection_source.as_str(),
            "payment detector failed while resolving invoice reference match for destination"
        );
        error
    })?;
    let any_reference_match = invoices::find_reference_match_any(pool, &transfer.account_keys)
        .await
        .map_err(|error| {
            tracing::error!(
                error = %error,
                tx_signature = %signature.signature,
                recipient_token_account = %recipient_token_account,
                token_mint = %token_mint,
                signal_source = detection_source.as_str(),
                "payment detector failed while resolving invoice reference match across invoices"
            );
            error
        })?;

    let settlement = resolve_reference_settlement(
        target_reference_match,
        any_reference_match,
        transfer.amount_usdc,
    );

    let invoice = match settlement {
        ReferenceSettlement::Matched(invoice) => invoice,
        ReferenceSettlement::Unmatched {
            reason,
            matched_invoice_id,
            reference_pubkey,
            invoice_status,
            expected_amount_usdc,
        } => {
            record_unmatched_payment(
                pool,
                &signature.signature,
                recipient_token_account,
                transfer.amount_usdc,
                transfer.source_owner.as_deref(),
                reference_pubkey.as_deref(),
                reason,
                matched_invoice_id,
                invoice_status.as_deref(),
                expected_amount_usdc,
                detection_source,
            )
            .await?;
            return Ok(());
        }
    };

    let payment_result = payments::create(
        pool,
        payments::CreatePayment {
            invoice_id: invoice.id,
            amount_usdc: transfer.amount_usdc.normalize().to_string(),
            tx_signature: signature.signature.clone(),
            payer_wallet_address: transfer.source_owner.clone(),
            finalized_at: Some(received_at),
            slot: Some(signature.slot),
        },
    )
    .await
    .map_err(|error| {
        tracing::error!(
            error = %error,
            tx_signature = %signature.signature,
            invoice_id = %invoice.id,
            recipient_token_account = %recipient_token_account,
            token_mint = %token_mint,
            signal_source = detection_source.as_str(),
            "payment detector failed while recording confirmed payment"
        );
        error
    })?;

    if !payment_result.inserted {
        tracing::info!(
            tx_signature = %signature.signature,
            signal_source = detection_source.as_str(),
            result = "ignored",
            reason = "already_processed",
            "detector ignored transaction"
        );
        return Ok(());
    }

    let payment = payment_result.payment;
    tracing::info!(
        tx_signature = %payment.tx_signature,
        invoice_id = %payment.invoice_id,
        amount_usdc = %payment.amount_usdc,
        match_strategy = "reference",
        signal_source = detection_source.as_str(),
        recipient_token_account = %payment.recipient_token_account,
        token_mint = %payment.token_mint,
        finalized_at = ?payment.finalized_at,
        slot = ?payment.slot,
        result = "paid",
        "detector marked invoice as paid from finalized USDC transfer"
    );

    Ok(())
}

async fn record_unmatched_payment(
    pool: &PgPool,
    tx_signature: &str,
    recipient_token_account: &str,
    amount_usdc: Decimal,
    sender_wallet: Option<&str>,
    reference_pubkey: Option<&str>,
    reason: UnmatchedReason,
    matched_invoice_id: Option<Uuid>,
    invoice_status: Option<&str>,
    expected_amount_usdc: Option<Decimal>,
    detection_source: DetectionSource,
) -> AppResult<()> {
    let inserted = unmatched_payments::create(
        pool,
        unmatched_payments::CreateUnmatchedPayment {
            signature: tx_signature.to_string(),
            destination_wallet: recipient_token_account.to_string(),
            amount_usdc: amount_usdc.normalize().to_string(),
            sender_wallet: sender_wallet.map(ToString::to_string),
            reference_pubkey: reference_pubkey.map(ToString::to_string),
            reason: reason.as_str().to_string(),
        },
    )
    .await?;

    tracing::warn!(
        tx_signature = %tx_signature,
        recipient_token_account = %recipient_token_account,
        amount_usdc = %amount_usdc,
        sender_wallet = ?sender_wallet,
        reference_pubkey = ?reference_pubkey,
        matched_invoice_id = ?matched_invoice_id,
        invoice_status = ?invoice_status,
        expected_amount_usdc = ?expected_amount_usdc,
        signal_source = detection_source.as_str(),
        result = "unmatched",
        reason = reason.as_str(),
        inserted,
        "detector recorded unmatched finalized USDC transfer"
    );

    Ok(())
}

fn resolve_reference_settlement(
    target_reference_match: Option<invoices::ReferenceMatchCandidate>,
    any_reference_match: Option<invoices::ReferenceMatchCandidate>,
    received_amount: Decimal,
) -> ReferenceSettlement {
    if let Some(invoice) = target_reference_match {
        if invoice.status != "pending" {
            return ReferenceSettlement::Unmatched {
                reason: UnmatchedReason::DuplicateOrLatePayment,
                matched_invoice_id: Some(invoice.id),
                reference_pubkey: Some(invoice.reference_pubkey),
                invoice_status: Some(invoice.status),
                expected_amount_usdc: Some(invoice.amount_usdc),
            };
        }

        if received_amount < invoice.amount_usdc {
            return ReferenceSettlement::Unmatched {
                reason: UnmatchedReason::AmountBelowInvoiceTotal,
                matched_invoice_id: Some(invoice.id),
                reference_pubkey: Some(invoice.reference_pubkey),
                invoice_status: Some(invoice.status),
                expected_amount_usdc: Some(invoice.amount_usdc),
            };
        }

        return ReferenceSettlement::Matched(invoice);
    }

    if let Some(invoice) = any_reference_match {
        return ReferenceSettlement::Unmatched {
            reason: UnmatchedReason::BadReference,
            matched_invoice_id: Some(invoice.id),
            reference_pubkey: Some(invoice.reference_pubkey),
            invoice_status: Some(invoice.status),
            expected_amount_usdc: Some(invoice.amount_usdc),
        };
    }

    ReferenceSettlement::Unmatched {
        reason: UnmatchedReason::MissingReference,
        matched_invoice_id: None,
        reference_pubkey: None,
        invoice_status: None,
        expected_amount_usdc: None,
    }
}

fn timestamp_to_utc(timestamp: Option<i64>) -> Option<DateTime<Utc>> {
    timestamp.and_then(|seconds| DateTime::<Utc>::from_timestamp(seconds, 0))
}

fn rate_limit_cooldown(
    poll_interval: Duration,
    retry_after: Duration,
    consecutive_rate_limits: u32,
) -> Duration {
    const MAX_RATE_LIMIT_COOLDOWN: Duration = Duration::from_secs(120);

    let multiplier = 1u32 << consecutive_rate_limits.min(4);
    let exponential = poll_interval
        .checked_mul(multiplier)
        .unwrap_or(MAX_RATE_LIMIT_COOLDOWN);

    retry_after.max(exponential).min(MAX_RATE_LIMIT_COOLDOWN)
}

fn websocket_backoff(current: Duration) -> Duration {
    const MAX_WEBSOCKET_RECONNECT_BACKOFF: Duration = Duration::from_secs(30);

    current
        .checked_mul(2)
        .unwrap_or(MAX_WEBSOCKET_RECONNECT_BACKOFF)
        .min(MAX_WEBSOCKET_RECONNECT_BACKOFF)
}

fn parse_logs_subscribe_ack(payload: &str) -> AppResult<Option<LogsSubscribeAck>> {
    let message: LogsWebsocketMessage =
        serde_json::from_str(payload).map_err(|error| AppError::Internal(anyhow::Error::new(error)))?;

    Ok(match message {
        LogsWebsocketMessage::SubscribeAck { id: Some(1), result } => {
            Some(LogsSubscribeAck { subscription_id: result })
        }
        LogsWebsocketMessage::SubscribeAck { .. }
        | LogsWebsocketMessage::Notification { .. }
        | LogsWebsocketMessage::Unknown => None,
    })
}

fn parse_logs_notification(payload: &str) -> AppResult<Option<LogsNotification>> {
    let message: LogsWebsocketMessage =
        serde_json::from_str(payload).map_err(|error| AppError::Internal(anyhow::Error::new(error)))?;

    Ok(match message {
        LogsWebsocketMessage::Notification { method, params }
            if method == "logsNotification" =>
        {
            Some(params.result)
        }
        LogsWebsocketMessage::SubscribeAck { .. }
        | LogsWebsocketMessage::Notification { .. }
        | LogsWebsocketMessage::Unknown => None,
    })
}

fn sanitize_pending_targets(
    targets: Vec<invoices::PendingSettlementTarget>,
) -> Vec<invoices::PendingSettlementTarget> {
    targets
        .into_iter()
        .filter(|target| match UsdcSettlement::from_wallet_pubkey(&target.wallet_pubkey) {
            Ok(settlement)
                if settlement.usdc_ata == target.usdc_ata
                    && settlement.usdc_mint == target.usdc_mint =>
            {
                true
            }
            Ok(settlement) => {
                tracing::warn!(
                    wallet_pubkey = %target.wallet_pubkey,
                    recipient_token_account = %target.usdc_ata,
                    expected_usdc_ata = %settlement.usdc_ata,
                    expected_usdc_mint = %settlement.usdc_mint,
                    signal_source = DetectionSource::Polling.as_str(),
                    "payment detector skipped invalid pending settlement target"
                );
                false
            }
            Err(error) => {
                tracing::warn!(
                    error = %error,
                    wallet_pubkey = %target.wallet_pubkey,
                    recipient_token_account = %target.usdc_ata,
                    signal_source = DetectionSource::Polling.as_str(),
                    "payment detector skipped pending settlement target with invalid wallet"
                );
                false
            }
        })
        .collect()
}

#[derive(Clone, Copy)]
enum DetectionSource {
    Polling,
    LogsSubscribe,
}

impl DetectionSource {
    fn as_str(self) -> &'static str {
        match self {
            Self::Polling => "polling",
            Self::LogsSubscribe => "logs_subscribe",
        }
    }
}

enum ReferenceSettlement {
    Matched(invoices::ReferenceMatchCandidate),
    Unmatched {
        reason: UnmatchedReason,
        matched_invoice_id: Option<Uuid>,
        reference_pubkey: Option<String>,
        invoice_status: Option<String>,
        expected_amount_usdc: Option<Decimal>,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum UnmatchedReason {
    MissingReference,
    BadReference,
    DuplicateOrLatePayment,
    AmountBelowInvoiceTotal,
}

impl UnmatchedReason {
    fn as_str(self) -> &'static str {
        match self {
            Self::MissingReference => "missing_reference",
            Self::BadReference => "bad_reference",
            Self::DuplicateOrLatePayment => "duplicate_or_late_payment",
            Self::AmountBelowInvoiceTotal => "amount_below_invoice_total",
        }
    }
}

struct LogsSubscribeAck {
    subscription_id: u64,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum LogsWebsocketMessage {
    SubscribeAck {
        result: u64,
        id: Option<u64>,
    },
    Notification {
        method: String,
        params: LogsNotificationParams,
    },
    Unknown,
}

#[derive(Deserialize)]
struct LogsNotificationParams {
    result: LogsNotification,
}

#[derive(Deserialize)]
struct LogsNotification {
    context: LogsContext,
    value: LogsNotificationValue,
}

#[derive(Deserialize)]
struct LogsContext {
    slot: i64,
}

#[derive(Deserialize)]
struct LogsNotificationValue {
    signature: String,
    err: Option<serde_json::Value>,
}

#[cfg(test)]
mod tests {
    use rust_decimal::Decimal;
    use uuid::Uuid;

    use super::{resolve_reference_settlement, ReferenceSettlement, UnmatchedReason};
    use crate::services::invoices::ReferenceMatchCandidate;

    fn candidate(status: &str, amount_usdc: Decimal) -> ReferenceMatchCandidate {
        ReferenceMatchCandidate {
            id: Uuid::new_v4(),
            reference_pubkey: Uuid::new_v4().simple().to_string(),
            amount_usdc,
            status: status.to_string(),
        }
    }

    #[test]
    fn missing_reference_stays_unmatched() {
        let result = resolve_reference_settlement(None, None, Decimal::new(100, 2));
        assert!(matches!(
            result,
            ReferenceSettlement::Unmatched {
                reason: UnmatchedReason::MissingReference,
                ..
            }
        ));
    }

    #[test]
    fn wrong_reference_stays_unmatched() {
        let result = resolve_reference_settlement(
            None,
            Some(candidate("pending", Decimal::new(100, 2))),
            Decimal::new(100, 2),
        );
        assert!(matches!(
            result,
            ReferenceSettlement::Unmatched {
                reason: UnmatchedReason::BadReference,
                ..
            }
        ));
    }

    #[test]
    fn underpaid_reference_stays_unmatched() {
        let result = resolve_reference_settlement(
            Some(candidate("pending", Decimal::new(250, 2))),
            None,
            Decimal::new(100, 2),
        );
        assert!(matches!(
            result,
            ReferenceSettlement::Unmatched {
                reason: UnmatchedReason::AmountBelowInvoiceTotal,
                ..
            }
        ));
    }

    #[test]
    fn paid_or_expired_reference_is_late_payment() {
        let result = resolve_reference_settlement(
            Some(candidate("paid", Decimal::new(100, 2))),
            None,
            Decimal::new(100, 2),
        );
        assert!(matches!(
            result,
            ReferenceSettlement::Unmatched {
                reason: UnmatchedReason::DuplicateOrLatePayment,
                ..
            }
        ));
    }

    #[test]
    fn pending_reference_with_sufficient_amount_matches() {
        let result = resolve_reference_settlement(
            Some(candidate("pending", Decimal::new(100, 2))),
            None,
            Decimal::new(125, 2),
        );
        assert!(matches!(result, ReferenceSettlement::Matched(_)));
    }
}
