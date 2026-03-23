use std::{
    collections::{HashMap, HashSet},
    time::Duration,
};

use chrono::{DateTime, Duration as ChronoDuration, Utc};
use futures_util::{SinkExt, StreamExt};
use serde::Deserialize;
use sqlx::PgPool;
use tokio::task::JoinHandle;
use tokio_tungstenite::{connect_async, tungstenite::Message};

use crate::{
    clients::solana::{SignatureInfo, SolanaRpcClient},
    error::{AppError, AppResult},
    services::{invoices, payments},
};

#[derive(Clone)]
pub struct PaymentDetectorConfig {
    pub poll_interval: Duration,
    pub match_window: ChronoDuration,
    pub signature_limit: usize,
}

#[derive(Default)]
struct DetectorCursor {
    last_seen_signature: Option<String>,
}

pub async fn run(
    pool: PgPool,
    solana: SolanaRpcClient,
    config: PaymentDetectorConfig,
) {
    let poll_pool = pool.clone();
    let poll_solana = solana.clone();
    let poll_config = config.clone();

    tokio::join!(
        run_poll_loop(poll_pool, poll_solana, poll_config),
        run_logs_manager_loop(pool, solana, config)
    );
}

async fn run_poll_loop(
    pool: PgPool,
    solana: SolanaRpcClient,
    config: PaymentDetectorConfig,
) {
    let mut cursors = HashMap::<String, DetectorCursor>::new();
    let mut consecutive_rate_limits = 0u32;

    loop {
        match poll_once(&pool, &solana, &config, &mut cursors).await {
            Ok(()) => {
                consecutive_rate_limits = 0;
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

async fn run_logs_manager_loop(
    pool: PgPool,
    solana: SolanaRpcClient,
    config: PaymentDetectorConfig,
) {
    let Some(ws_url) = solana.websocket_url().map(str::to_string) else {
        tracing::warn!("payment detector websocket disabled: unable to derive websocket URL from RPC URL");
        return;
    };
    let mut subscriptions = HashMap::<String, JoinHandle<()>>::new();

    loop {
        match invoices::list_pending_settlement_targets(&pool).await {
            Ok(targets) => {
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
                    let task_config = config.clone();
                    let ws_url = ws_url.clone();
                    let recipient_token_account = target.usdc_ata.clone();
                    let token_mint = target.usdc_mint.clone();

                    let handle = tokio::spawn(async move {
                        run_logs_subscription_loop_for_target(
                            task_pool,
                            task_solana,
                            task_config,
                            ws_url,
                            recipient_token_account,
                            token_mint,
                        )
                        .await;
                    });

                    subscriptions.insert(target.usdc_ata, handle);
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
    config: PaymentDetectorConfig,
    ws_url: String,
    recipient_token_account: String,
    token_mint: String,
) {
    let mut reconnect_backoff = Duration::from_secs(1);

    loop {
        match consume_logs_subscription(
            &pool,
            &solana,
            &config,
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
) -> AppResult<()> {
    let targets = invoices::list_pending_settlement_targets(pool).await?;
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

        let newest_signature = signatures.first().map(|signature| signature.signature.clone());
        signatures.reverse();

        for signature in signatures {
            process_signature(
                pool,
                solana,
                &target.usdc_ata,
                &target.usdc_mint,
                config,
                &signature,
                DetectionSource::Polling,
            )
            .await?;
        }

        cursor.last_seen_signature = newest_signature;
    }

    Ok(())
}

async fn consume_logs_subscription(
    pool: &PgPool,
    solana: &SolanaRpcClient,
    config: &PaymentDetectorConfig,
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
                    config,
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
    config: &PaymentDetectorConfig,
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

    let Some(transfer) = solana
        .get_finalized_usdc_transfer_to_token_account(
            &signature.signature,
            recipient_token_account,
            token_mint,
        )
        .await?
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
    let window_start = received_at - config.match_window;

    let match_result = if let Some(invoice) =
        invoices::find_pending_match_by_reference(pool, &transfer.account_keys).await?
    {
        Some((invoice, "reference"))
    } else {
        invoices::find_pending_match(
            pool,
            recipient_token_account,
            token_mint,
            transfer.amount_usdc,
            window_start,
            received_at,
        )
        .await?
        .map(|invoice| (invoice, "amount_fallback"))
    };

    let Some((invoice, match_strategy)) = match_result else {
        tracing::info!(
            tx_signature = %signature.signature,
            recipient_token_account = %recipient_token_account,
            token_mint = %token_mint,
            amount_usdc = %transfer.amount_usdc,
            signal_source = detection_source.as_str(),
            result = "ignored",
            reason = "no_pending_invoice_match",
            "detector found a finalized USDC transfer but no pending invoice match"
        );
        return Ok(());
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
    .await?;

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
        match_strategy,
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
