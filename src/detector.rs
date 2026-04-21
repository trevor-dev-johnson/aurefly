use std::{
    collections::{HashMap, HashSet},
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
    time::Duration,
};

use chrono::{DateTime, Utc};
use futures_util::{SinkExt, StreamExt};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use tokio::task::JoinHandle;
use tokio::time::Instant;
use tokio_tungstenite::{connect_async, tungstenite::Message};
use uuid::Uuid;

use crate::{
    clients::solana::{SignatureInfo, SolanaRpcClient},
    error::{AppError, AppResult},
    services::{invoices, payments, unmatched_payments},
    solana::UsdcSettlement,
};

#[derive(Clone)]
pub struct PaymentDetectorConfig {
    pub scheduler_tick: Duration,
    pub fast_poll_interval: Duration,
    pub medium_poll_interval: Duration,
    pub slow_poll_interval: Duration,
    pub fast_window: Duration,
    pub medium_window: Duration,
    pub max_targets_per_cycle: usize,
    pub max_active_logs_subscriptions: usize,
    pub max_idle_backoff: Duration,
    pub signature_dedupe_ttl: Duration,
    pub signature_limit: usize,
    pub pending_invoice_ttl: Duration,
}

#[derive(Default)]
struct DetectorCursor {
    last_seen_signature: Option<String>,
    idle_poll_streak: u32,
    next_poll_after: Option<Instant>,
}

#[derive(Default)]
struct PollStats {
    pending_target_count: usize,
    due_target_count: usize,
    deferred_target_count: usize,
    skipped_not_due_count: usize,
    new_signature_count: usize,
    processed_signature_count: usize,
}

const DETECTOR_HEARTBEAT_INTERVAL: Duration = Duration::from_secs(15);

#[derive(Default)]
struct DetectorMetrics {
    poll_cycles: AtomicU64,
    target_checks_started: AtomicU64,
    target_checks_skipped_not_due: AtomicU64,
    target_checks_deferred: AtomicU64,
    signatures_seen: AtomicU64,
    signatures_processed: AtomicU64,
    matched_payments: AtomicU64,
    unmatched_payments: AtomicU64,
    duplicate_detection_attempts: AtomicU64,
    finalized_events_seen: AtomicU64,
    ignored_non_finalized: AtomicU64,
    rpc_rate_limits: AtomicU64,
    rpc_failures: AtomicU64,
    websocket_notifications: AtomicU64,
    polling_notifications: AtomicU64,
    detection_latency_ms_total: AtomicU64,
    detection_latency_samples: AtomicU64,
}

#[derive(Clone, Copy, Default)]
struct DetectorMetricsSnapshot {
    poll_cycles: u64,
    target_checks_started: u64,
    target_checks_skipped_not_due: u64,
    target_checks_deferred: u64,
    signatures_seen: u64,
    signatures_processed: u64,
    matched_payments: u64,
    unmatched_payments: u64,
    duplicate_detection_attempts: u64,
    finalized_events_seen: u64,
    ignored_non_finalized: u64,
    rpc_rate_limits: u64,
    rpc_failures: u64,
    websocket_notifications: u64,
    polling_notifications: u64,
    detection_latency_ms_total: u64,
    detection_latency_samples: u64,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct DetectorRuntimeSnapshot {
    pub started_at: Option<DateTime<Utc>>,
    pub last_heartbeat_at: Option<DateTime<Utc>>,
    pub rpc_url: String,
    pub fallback_rpc_url: Option<String>,
    pub websocket_enabled: bool,
    pub scheduler_tick_secs: u64,
    pub fast_poll_interval_secs: u64,
    pub medium_poll_interval_secs: u64,
    pub slow_poll_interval_secs: u64,
    pub fast_window_secs: u64,
    pub medium_window_secs: u64,
    pub max_targets_per_cycle: usize,
    pub max_active_logs_subscriptions: usize,
    pub max_idle_backoff_secs: u64,
    pub signature_dedupe_ttl_secs: u64,
    pub signature_limit: usize,
    pub pending_invoice_ttl_secs: u64,
    pub pending_target_count: usize,
    pub active_logs_target_count: usize,
    pub checks_per_minute: f64,
    pub checks_per_invoice: f64,
    pub avg_detection_secs: Option<f64>,
    pub interval_target_checks: u64,
    pub interval_matched_payments: u64,
    pub interval_unmatched_payments: u64,
    pub interval_rpc_rate_limits: u64,
    pub interval_rpc_failures: u64,
    pub interval_websocket_notifications: u64,
    pub interval_polling_notifications: u64,
    pub total_target_checks: u64,
    pub total_matched_payments: u64,
    pub total_unmatched_payments: u64,
    pub total_rpc_rate_limits: u64,
    pub total_rpc_failures: u64,
    pub total_duplicate_detection_attempts: u64,
}

#[derive(Clone, Default)]
pub struct DetectorRuntime {
    inner: Arc<tokio::sync::RwLock<DetectorRuntimeSnapshot>>,
}

impl DetectorRuntime {
    pub async fn snapshot(&self) -> DetectorRuntimeSnapshot {
        self.inner.read().await.clone()
    }

    async fn mark_started(
        &self,
        solana: &SolanaRpcClient,
        config: &PaymentDetectorConfig,
        startup_target_count: usize,
    ) {
        let mut snapshot = self.inner.write().await;
        *snapshot = DetectorRuntimeSnapshot {
            started_at: Some(Utc::now()),
            last_heartbeat_at: None,
            rpc_url: solana.redacted_rpc_url(),
            fallback_rpc_url: solana.redacted_fallback_rpc_url(),
            websocket_enabled: solana.websocket_url().is_some(),
            scheduler_tick_secs: config.scheduler_tick.as_secs(),
            fast_poll_interval_secs: config.fast_poll_interval.as_secs(),
            medium_poll_interval_secs: config.medium_poll_interval.as_secs(),
            slow_poll_interval_secs: config.slow_poll_interval.as_secs(),
            fast_window_secs: config.fast_window.as_secs(),
            medium_window_secs: config.medium_window.as_secs(),
            max_targets_per_cycle: config.max_targets_per_cycle,
            max_active_logs_subscriptions: config.max_active_logs_subscriptions,
            max_idle_backoff_secs: config.max_idle_backoff.as_secs(),
            signature_dedupe_ttl_secs: config.signature_dedupe_ttl.as_secs(),
            signature_limit: config.signature_limit,
            pending_invoice_ttl_secs: config.pending_invoice_ttl.as_secs(),
            pending_target_count: startup_target_count,
            active_logs_target_count: 0,
            checks_per_minute: 0.0,
            checks_per_invoice: 0.0,
            avg_detection_secs: None,
            interval_target_checks: 0,
            interval_matched_payments: 0,
            interval_unmatched_payments: 0,
            interval_rpc_rate_limits: 0,
            interval_rpc_failures: 0,
            interval_websocket_notifications: 0,
            interval_polling_notifications: 0,
            total_target_checks: 0,
            total_matched_payments: 0,
            total_unmatched_payments: 0,
            total_rpc_rate_limits: 0,
            total_rpc_failures: 0,
            total_duplicate_detection_attempts: 0,
        };
    }

    async fn update_heartbeat(
        &self,
        pending_target_count: usize,
        active_logs_target_count: usize,
        interval_metrics: DetectorMetricsSnapshot,
        total_metrics: DetectorMetricsSnapshot,
        checks_per_minute: f64,
        checks_per_invoice: f64,
    ) {
        let mut snapshot = self.inner.write().await;
        snapshot.last_heartbeat_at = Some(Utc::now());
        snapshot.pending_target_count = pending_target_count;
        snapshot.active_logs_target_count = active_logs_target_count;
        snapshot.checks_per_minute = checks_per_minute;
        snapshot.checks_per_invoice = checks_per_invoice;
        snapshot.avg_detection_secs = interval_metrics.average_detection_secs();
        snapshot.interval_target_checks = interval_metrics.target_checks_started;
        snapshot.interval_matched_payments = interval_metrics.matched_payments;
        snapshot.interval_unmatched_payments = interval_metrics.unmatched_payments;
        snapshot.interval_rpc_rate_limits = interval_metrics.rpc_rate_limits;
        snapshot.interval_rpc_failures = interval_metrics.rpc_failures;
        snapshot.interval_websocket_notifications = interval_metrics.websocket_notifications;
        snapshot.interval_polling_notifications = interval_metrics.polling_notifications;
        snapshot.total_target_checks = total_metrics.target_checks_started;
        snapshot.total_matched_payments = total_metrics.matched_payments;
        snapshot.total_unmatched_payments = total_metrics.unmatched_payments;
        snapshot.total_rpc_rate_limits = total_metrics.rpc_rate_limits;
        snapshot.total_rpc_failures = total_metrics.rpc_failures;
        snapshot.total_duplicate_detection_attempts = total_metrics.duplicate_detection_attempts;
    }
}

#[derive(Clone, Default)]
struct DetectorShared {
    metrics: Arc<DetectorMetrics>,
    active_logs_targets: Arc<tokio::sync::RwLock<HashSet<String>>>,
    claimed_signatures: Arc<tokio::sync::Mutex<HashMap<String, Instant>>>,
    runtime: DetectorRuntime,
}

impl DetectorMetrics {
    fn snapshot(&self) -> DetectorMetricsSnapshot {
        DetectorMetricsSnapshot {
            poll_cycles: self.poll_cycles.load(Ordering::Relaxed),
            target_checks_started: self.target_checks_started.load(Ordering::Relaxed),
            target_checks_skipped_not_due: self
                .target_checks_skipped_not_due
                .load(Ordering::Relaxed),
            target_checks_deferred: self.target_checks_deferred.load(Ordering::Relaxed),
            signatures_seen: self.signatures_seen.load(Ordering::Relaxed),
            signatures_processed: self.signatures_processed.load(Ordering::Relaxed),
            matched_payments: self.matched_payments.load(Ordering::Relaxed),
            unmatched_payments: self.unmatched_payments.load(Ordering::Relaxed),
            duplicate_detection_attempts: self.duplicate_detection_attempts.load(Ordering::Relaxed),
            finalized_events_seen: self.finalized_events_seen.load(Ordering::Relaxed),
            ignored_non_finalized: self.ignored_non_finalized.load(Ordering::Relaxed),
            rpc_rate_limits: self.rpc_rate_limits.load(Ordering::Relaxed),
            rpc_failures: self.rpc_failures.load(Ordering::Relaxed),
            websocket_notifications: self.websocket_notifications.load(Ordering::Relaxed),
            polling_notifications: self.polling_notifications.load(Ordering::Relaxed),
            detection_latency_ms_total: self.detection_latency_ms_total.load(Ordering::Relaxed),
            detection_latency_samples: self.detection_latency_samples.load(Ordering::Relaxed),
        }
    }
}

impl DetectorMetricsSnapshot {
    fn delta_since(self, previous: Self) -> Self {
        Self {
            poll_cycles: self.poll_cycles.saturating_sub(previous.poll_cycles),
            target_checks_started: self
                .target_checks_started
                .saturating_sub(previous.target_checks_started),
            target_checks_skipped_not_due: self
                .target_checks_skipped_not_due
                .saturating_sub(previous.target_checks_skipped_not_due),
            target_checks_deferred: self
                .target_checks_deferred
                .saturating_sub(previous.target_checks_deferred),
            signatures_seen: self
                .signatures_seen
                .saturating_sub(previous.signatures_seen),
            signatures_processed: self
                .signatures_processed
                .saturating_sub(previous.signatures_processed),
            matched_payments: self
                .matched_payments
                .saturating_sub(previous.matched_payments),
            unmatched_payments: self
                .unmatched_payments
                .saturating_sub(previous.unmatched_payments),
            duplicate_detection_attempts: self
                .duplicate_detection_attempts
                .saturating_sub(previous.duplicate_detection_attempts),
            finalized_events_seen: self
                .finalized_events_seen
                .saturating_sub(previous.finalized_events_seen),
            ignored_non_finalized: self
                .ignored_non_finalized
                .saturating_sub(previous.ignored_non_finalized),
            rpc_rate_limits: self
                .rpc_rate_limits
                .saturating_sub(previous.rpc_rate_limits),
            rpc_failures: self.rpc_failures.saturating_sub(previous.rpc_failures),
            websocket_notifications: self
                .websocket_notifications
                .saturating_sub(previous.websocket_notifications),
            polling_notifications: self
                .polling_notifications
                .saturating_sub(previous.polling_notifications),
            detection_latency_ms_total: self
                .detection_latency_ms_total
                .saturating_sub(previous.detection_latency_ms_total),
            detection_latency_samples: self
                .detection_latency_samples
                .saturating_sub(previous.detection_latency_samples),
        }
    }

    fn average_detection_secs(self) -> Option<f64> {
        if self.detection_latency_samples == 0 {
            None
        } else {
            Some(
                self.detection_latency_ms_total as f64
                    / self.detection_latency_samples as f64
                    / 1000.0,
            )
        }
    }
}

impl DetectorShared {
    fn with_runtime(runtime: DetectorRuntime) -> Self {
        Self { runtime, ..Self::default() }
    }

    async fn set_active_logs_targets(&self, targets: HashSet<String>) {
        *self.active_logs_targets.write().await = targets;
    }

    async fn active_logs_target_count(&self) -> usize {
        self.active_logs_targets.read().await.len()
    }

    async fn has_active_logs_target(&self, target: &str) -> bool {
        self.active_logs_targets.read().await.contains(target)
    }

    async fn try_claim_signature(&self, signature: &str, ttl: Duration) -> bool {
        let now = Instant::now();
        let mut claimed = self.claimed_signatures.lock().await;
        claimed.retain(|_, seen_at| now.duration_since(*seen_at) <= ttl);

        if claimed.contains_key(signature) {
            return false;
        }

        claimed.insert(signature.to_string(), now);
        true
    }

    async fn release_signature(&self, signature: &str) {
        self.claimed_signatures.lock().await.remove(signature);
    }
}

pub async fn run(
    pool: PgPool,
    solana: SolanaRpcClient,
    config: PaymentDetectorConfig,
    runtime: DetectorRuntime,
) {
    let shared = DetectorShared::with_runtime(runtime.clone());
    let startup_targets = load_pending_targets(&pool, config.pending_invoice_ttl).await;
    runtime
        .mark_started(&solana, &config, startup_targets.len())
        .await;
    tracing::info!(
        rpc = %solana.redacted_rpc_url(),
        fallback_rpc = ?solana.redacted_fallback_rpc_url(),
        ata_count = startup_targets.len(),
        ata = %format_target_preview(&startup_targets),
        scheduler_tick_secs = config.scheduler_tick.as_secs(),
        fast_poll_interval_secs = config.fast_poll_interval.as_secs(),
        medium_poll_interval_secs = config.medium_poll_interval.as_secs(),
        slow_poll_interval_secs = config.slow_poll_interval.as_secs(),
        fast_window_secs = config.fast_window.as_secs(),
        medium_window_secs = config.medium_window.as_secs(),
        max_targets_per_cycle = config.max_targets_per_cycle,
        max_active_logs_subscriptions = config.max_active_logs_subscriptions,
        max_idle_backoff_secs = config.max_idle_backoff.as_secs(),
        signature_dedupe_ttl_secs = config.signature_dedupe_ttl.as_secs(),
        signature_limit = config.signature_limit,
        pending_invoice_ttl_secs = config.pending_invoice_ttl.as_secs(),
        websocket_enabled = solana.websocket_url().is_some(),
        "[detector] started"
    );
    tracing::info!(
        signal_source = DetectionSource::Polling.as_str(),
        scheduler_tick_secs = config.scheduler_tick.as_secs(),
        fast_poll_interval_secs = config.fast_poll_interval.as_secs(),
        medium_poll_interval_secs = config.medium_poll_interval.as_secs(),
        slow_poll_interval_secs = config.slow_poll_interval.as_secs(),
        signature_limit = config.signature_limit,
        max_targets_per_cycle = config.max_targets_per_cycle,
        "payment detector polling loop started"
    );
    tracing::info!(
        signal_source = DetectionSource::LogsSubscribe.as_str(),
        websocket_enabled = solana.websocket_url().is_some(),
        max_active_logs_subscriptions = config.max_active_logs_subscriptions,
        "payment detector websocket manager started"
    );

    let poll_pool = pool.clone();
    let poll_solana = solana.clone();
    let poll_config = config.clone();
    let poll_shared = shared.clone();

    tokio::join!(
        run_heartbeat_loop(
            pool.clone(),
            solana.clone(),
            config.pending_invoice_ttl,
            shared.clone()
        ),
        run_poll_loop(poll_pool, poll_solana, poll_config, poll_shared),
        run_logs_manager_loop(pool, solana, config, shared)
    );
}

async fn run_heartbeat_loop(
    pool: PgPool,
    solana: SolanaRpcClient,
    pending_invoice_ttl: Duration,
    shared: DetectorShared,
) {
    let mut previous_metrics = shared.metrics.snapshot();

    loop {
        tokio::time::sleep(DETECTOR_HEARTBEAT_INTERVAL).await;
        let targets = load_pending_targets(&pool, pending_invoice_ttl).await;
        let active_logs_subscriptions = shared.active_logs_target_count().await;
        let current_metrics = shared.metrics.snapshot();
        let interval_metrics = current_metrics.delta_since(previous_metrics);
        previous_metrics = current_metrics;
        let heartbeat_secs = DETECTOR_HEARTBEAT_INTERVAL.as_secs().max(1) as f64;
        let checks_per_minute =
            (interval_metrics.target_checks_started as f64 / heartbeat_secs) * 60.0;
        let checks_per_invoice = if targets.is_empty() {
            0.0
        } else {
            interval_metrics.target_checks_started as f64 / targets.len() as f64
        };
        shared
            .runtime
            .update_heartbeat(
                targets.len(),
                active_logs_subscriptions,
                interval_metrics,
                current_metrics,
                checks_per_minute,
                checks_per_invoice,
            )
            .await;

        tracing::info!(
            rpc = %solana.redacted_rpc_url(),
            ata_count = targets.len(),
            ata = %format_target_preview(&targets),
            active_logs_subscriptions,
            checks_per_minute,
            checks_per_invoice,
            interval_target_checks = interval_metrics.target_checks_started,
            interval_skipped_not_due = interval_metrics.target_checks_skipped_not_due,
            interval_deferred = interval_metrics.target_checks_deferred,
            interval_signatures_seen = interval_metrics.signatures_seen,
            interval_signatures_processed = interval_metrics.signatures_processed,
            interval_matched_payments = interval_metrics.matched_payments,
            interval_unmatched_payments = interval_metrics.unmatched_payments,
            interval_rpc_rate_limits = interval_metrics.rpc_rate_limits,
            interval_rpc_failures = interval_metrics.rpc_failures,
            interval_duplicate_detection_attempts =
                interval_metrics.duplicate_detection_attempts,
            interval_finalized_events_seen = interval_metrics.finalized_events_seen,
            interval_seen_but_not_finalized = interval_metrics.ignored_non_finalized,
            interval_websocket_notifications = interval_metrics.websocket_notifications,
            interval_polling_notifications = interval_metrics.polling_notifications,
            avg_detection_secs = interval_metrics.average_detection_secs(),
            total_target_checks = current_metrics.target_checks_started,
            total_matched_payments = current_metrics.matched_payments,
            total_unmatched_payments = current_metrics.unmatched_payments,
            "[detector] alive"
        );
    }
}

async fn run_poll_loop(
    pool: PgPool,
    solana: SolanaRpcClient,
    config: PaymentDetectorConfig,
    shared: DetectorShared,
) {
    let mut cursors = HashMap::<String, DetectorCursor>::new();
    let mut consecutive_rate_limits = 0u32;

    loop {
        match poll_once(&pool, &solana, &config, &mut cursors, &shared).await {
            Ok(stats) => {
                consecutive_rate_limits = 0;
                shared.metrics.poll_cycles.fetch_add(1, Ordering::Relaxed);
                shared
                    .metrics
                    .target_checks_skipped_not_due
                    .fetch_add(stats.skipped_not_due_count as u64, Ordering::Relaxed);
                shared
                    .metrics
                    .target_checks_deferred
                    .fetch_add(stats.deferred_target_count as u64, Ordering::Relaxed);
                tracing::debug!(
                    signal_source = DetectionSource::Polling.as_str(),
                    pending_targets = stats.pending_target_count,
                    due_targets = stats.due_target_count,
                    deferred_targets = stats.deferred_target_count,
                    skipped_not_due = stats.skipped_not_due_count,
                    tracked_cursors = cursors.len(),
                    new_signatures = stats.new_signature_count,
                    processed_signatures = stats.processed_signature_count,
                    scheduler_tick_secs = config.scheduler_tick.as_secs(),
                    "payment detector poll cycle completed"
                );
                tokio::time::sleep(config.scheduler_tick).await;
            }
            Err(error @ AppError::RateLimited { .. }) => {
                consecutive_rate_limits = consecutive_rate_limits.saturating_add(1);
                shared
                    .metrics
                    .rpc_rate_limits
                    .fetch_add(1, Ordering::Relaxed);
                let retry_after = error.retry_after().unwrap_or(config.medium_poll_interval);
                let cooldown = rate_limit_cooldown(
                    config.medium_poll_interval,
                    retry_after,
                    consecutive_rate_limits,
                );

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
                shared.metrics.rpc_failures.fetch_add(1, Ordering::Relaxed);
                tracing::error!(error = %error, "payment detector cycle failed");
                tokio::time::sleep(config.scheduler_tick).await;
            }
        }
    }
}

async fn run_logs_manager_loop(
    pool: PgPool,
    solana: SolanaRpcClient,
    config: PaymentDetectorConfig,
    shared: DetectorShared,
) {
    let ws_urls = solana.websocket_urls();
    if ws_urls.is_empty() {
        tracing::warn!(
            "payment detector websocket disabled: unable to derive websocket URL from RPC URL"
        );
        return;
    }
    let mut subscriptions = HashMap::<String, JoinHandle<()>>::new();

    loop {
        let mut all_targets = load_pending_targets(&pool, config.pending_invoice_ttl).await;
        all_targets.sort_by(target_priority_cmp);
        let active_targets = all_targets
            .iter()
            .map(|target| target.usdc_ata.clone())
            .collect::<HashSet<_>>();
        let recent_targets = all_targets
            .into_iter()
            .filter(|target| target_age(target) <= config.medium_window)
            .take(config.max_active_logs_subscriptions)
            .collect::<Vec<_>>();
        let desired_targets = recent_targets
            .iter()
            .map(|target| target.usdc_ata.clone())
            .collect::<HashSet<_>>();
        shared
            .set_active_logs_targets(desired_targets.clone())
            .await;

        let stale_targets = subscriptions
            .keys()
            .filter(|target| !desired_targets.contains(*target))
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

        for target in recent_targets {
            if subscriptions.contains_key(&target.usdc_ata) {
                continue;
            }

            let task_pool = pool.clone();
            let task_solana = solana.clone();
            let ws_urls = ws_urls.clone();
            let recipient_token_account = target.usdc_ata.clone();
            let token_mint = target.usdc_mint.clone();

            let task_shared = shared.clone();
            let task_config = config.clone();
            let handle = tokio::spawn(async move {
                run_logs_subscription_loop_for_target(
                    task_pool,
                    task_solana,
                    ws_urls,
                    recipient_token_account,
                    token_mint,
                    task_shared,
                    task_config,
                )
                .await;
            });

            subscriptions.insert(target.usdc_ata, handle);
        }
        tracing::debug!(
            signal_source = DetectionSource::LogsSubscribe.as_str(),
            pending_targets = active_targets.len(),
            websocket_candidates = desired_targets.len(),
            active_subscriptions = subscriptions.len(),
            "payment detector websocket target refresh completed"
        );

        tokio::time::sleep(Duration::from_secs(10)).await;
    }
}

async fn run_logs_subscription_loop_for_target(
    pool: PgPool,
    solana: SolanaRpcClient,
    ws_urls: Vec<String>,
    recipient_token_account: String,
    token_mint: String,
    shared: DetectorShared,
    config: PaymentDetectorConfig,
) {
    let mut reconnect_backoff = Duration::from_secs(1);
    let mut ws_index = 0usize;

    loop {
        let ws_url = &ws_urls[ws_index % ws_urls.len()];
        match consume_logs_subscription(
            &pool,
            &solana,
            ws_url,
            &recipient_token_account,
            &token_mint,
            &shared,
            &config,
        )
        .await
        {
            Ok(()) => {
                reconnect_backoff = Duration::from_secs(1);
                tracing::warn!(
                    recipient_token_account = %recipient_token_account,
                    websocket_url = %redact_ws_url(ws_url),
                    reconnect_delay_secs = reconnect_backoff.as_secs(),
                    signal_source = DetectionSource::LogsSubscribe.as_str(),
                    "payment detector logs subscription ended; reconnecting"
                );
            }
            Err(error) => {
                shared.metrics.rpc_failures.fetch_add(1, Ordering::Relaxed);
                tracing::warn!(
                    error = %error,
                    recipient_token_account = %recipient_token_account,
                    websocket_url = %redact_ws_url(ws_url),
                    reconnect_delay_secs = reconnect_backoff.as_secs(),
                    signal_source = DetectionSource::LogsSubscribe.as_str(),
                    "payment detector logs subscription failed; reconnecting"
                );
            }
        }

        tokio::time::sleep(reconnect_backoff).await;
        reconnect_backoff = websocket_backoff(reconnect_backoff);
        ws_index = ws_index.wrapping_add(1);
    }
}

async fn poll_once(
    pool: &PgPool,
    solana: &SolanaRpcClient,
    config: &PaymentDetectorConfig,
    cursors: &mut HashMap<String, DetectorCursor>,
    shared: &DetectorShared,
) -> AppResult<PollStats> {
    let targets = load_pending_targets(pool, config.pending_invoice_ttl).await;
    let now = Instant::now();
    let mut stats = PollStats {
        pending_target_count: targets.len(),
        ..PollStats::default()
    };
    let active_targets = targets
        .iter()
        .map(|target| target.usdc_ata.clone())
        .collect::<HashSet<_>>();
    cursors.retain(|target, _| active_targets.contains(target));

    let mut due_targets = Vec::new();

    for target in targets {
        let cursor = cursors.entry(target.usdc_ata.clone()).or_default();
        if let Some(next_poll_after) = cursor.next_poll_after {
            if next_poll_after > now {
                stats.skipped_not_due_count += 1;
                continue;
            }
        }
        due_targets.push(target);
    }

    due_targets.sort_by(target_priority_cmp);
    stats.due_target_count = due_targets.len();

    let overflow_targets = due_targets
        .iter()
        .skip(config.max_targets_per_cycle)
        .cloned()
        .collect::<Vec<_>>();
    for target in overflow_targets {
        let cursor = cursors.entry(target.usdc_ata.clone()).or_default();
        let has_active_logs_target = shared.has_active_logs_target(&target.usdc_ata).await;
        cursor.next_poll_after = Some(
            now + next_poll_delay(
                config,
                &target,
                cursor.idle_poll_streak.saturating_add(1),
                has_active_logs_target,
            ),
        );
        stats.deferred_target_count += 1;
    }

    for target in due_targets.into_iter().take(config.max_targets_per_cycle) {
        let cursor = cursors.entry(target.usdc_ata.clone()).or_default();
        let has_active_logs_target = shared.has_active_logs_target(&target.usdc_ata).await;
        shared
            .metrics
            .target_checks_started
            .fetch_add(1, Ordering::Relaxed);
        let mut signatures = solana
            .get_finalized_signatures_for_address(
                &target.usdc_ata,
                config.signature_limit,
                cursor.last_seen_signature.as_deref(),
            )
            .await?;

        if signatures.is_empty() {
            cursor.idle_poll_streak = cursor.idle_poll_streak.saturating_add(1);
            cursor.next_poll_after = Some(
                now + next_poll_delay(
                    config,
                    &target,
                    cursor.idle_poll_streak,
                    has_active_logs_target,
                ),
            );
            continue;
        }

        cursor.idle_poll_streak = 0;
        cursor.next_poll_after =
            Some(now + target_base_poll_interval(config, &target, has_active_logs_target));

        tracing::info!(
            recipient_token_account = %target.usdc_ata,
            token_mint = %target.usdc_mint,
            open_invoice_count = target.open_invoice_count,
            newest_invoice_age_secs = target_age(&target).as_secs(),
            has_active_logs_target,
            new_signatures = signatures.len(),
            signal_source = DetectionSource::Polling.as_str(),
            "payment detector found finalized signatures to inspect"
        );
        shared
            .metrics
            .polling_notifications
            .fetch_add(signatures.len() as u64, Ordering::Relaxed);
        shared
            .metrics
            .signatures_seen
            .fetch_add(signatures.len() as u64, Ordering::Relaxed);
        stats.new_signature_count += signatures.len();

        let newest_signature = signatures
            .first()
            .map(|signature| signature.signature.clone());
        signatures.reverse();

        for signature in signatures {
            process_signature(
                pool,
                solana,
                &target.usdc_ata,
                &target.usdc_mint,
                &signature,
                DetectionSource::Polling,
                config,
                shared,
            )
            .await?;
            stats.processed_signature_count += 1;
            shared
                .metrics
                .signatures_processed
                .fetch_add(1, Ordering::Relaxed);
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
    shared: &DetectorShared,
    config: &PaymentDetectorConfig,
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
        websocket_url = %redact_ws_url(ws_url),
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
                shared
                    .metrics
                    .websocket_notifications
                    .fetch_add(1, Ordering::Relaxed);
                shared
                    .metrics
                    .signatures_seen
                    .fetch_add(1, Ordering::Relaxed);

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
                    config,
                    shared,
                )
                .await?;
                shared
                    .metrics
                    .signatures_processed
                    .fetch_add(1, Ordering::Relaxed);
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
    config: &PaymentDetectorConfig,
    shared: &DetectorShared,
) -> AppResult<()> {
    if !shared
        .try_claim_signature(&signature.signature, config.signature_dedupe_ttl)
        .await
    {
        shared
            .metrics
            .duplicate_detection_attempts
            .fetch_add(1, Ordering::Relaxed);
        tracing::debug!(
            tx_signature = %signature.signature,
            signal_source = detection_source.as_str(),
            result = "ignored",
            reason = "duplicate_detection_attempt",
            "detector skipped duplicate transaction check"
        );
        return Ok(());
    }

    if signature.confirmation_status.as_deref() != Some("finalized") {
        shared
            .metrics
            .ignored_non_finalized
            .fetch_add(1, Ordering::Relaxed);
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

    let already_processed = match payments::tx_signature_exists(pool, &signature.signature).await {
        Ok(value) => value,
        Err(error) => {
            shared.release_signature(&signature.signature).await;
            return Err(error);
        }
    };
    if already_processed {
        shared
            .metrics
            .duplicate_detection_attempts
            .fetch_add(1, Ordering::Relaxed);
        tracing::info!(
            tx_signature = %signature.signature,
            signal_source = detection_source.as_str(),
            result = "ignored",
            reason = "already_processed",
            "detector ignored transaction"
        );
        return Ok(());
    }
    shared
        .metrics
        .finalized_events_seen
        .fetch_add(1, Ordering::Relaxed);

    let transfer = match solana
        .get_finalized_usdc_transfer_to_token_account(
            &signature.signature,
            recipient_token_account,
            token_mint,
        )
        .await
        .map_err(|error| {
            match error {
                AppError::RateLimited { .. } => {
                    shared
                        .metrics
                        .rpc_rate_limits
                        .fetch_add(1, Ordering::Relaxed);
                }
                _ => {
                    shared.metrics.rpc_failures.fetch_add(1, Ordering::Relaxed);
                }
            }
            tracing::warn!(
                error = %error,
                tx_signature = %signature.signature,
                recipient_token_account = %recipient_token_account,
                token_mint = %token_mint,
                signal_source = detection_source.as_str(),
                "payment detector failed while fetching finalized transaction details"
            );
            error
        }) {
        Ok(transfer) => transfer,
        Err(error) => {
            shared.release_signature(&signature.signature).await;
            return Err(error);
        }
    };
    let Some(transfer) = transfer else {
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
    let detection_latency_ms = transfer
        .finalized_at
        .map(|finalized_at| (Utc::now() - finalized_at).num_milliseconds().max(0) as u64);

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
    .await;
    let target_reference_match = match target_reference_match {
        Ok(invoice) => invoice,
        Err(error) => {
            shared.release_signature(&signature.signature).await;
            tracing::error!(
                error = %error,
                tx_signature = %signature.signature,
                recipient_token_account = %recipient_token_account,
                token_mint = %token_mint,
                signal_source = detection_source.as_str(),
                "payment detector failed while resolving invoice reference match for destination"
            );
            return Err(error);
        }
    };
    let any_reference_match =
        invoices::find_reference_match_any(pool, &transfer.account_keys).await;
    let any_reference_match = match any_reference_match {
        Ok(invoice) => invoice,
        Err(error) => {
            shared.release_signature(&signature.signature).await;
            tracing::error!(
                error = %error,
                tx_signature = %signature.signature,
                recipient_token_account = %recipient_token_account,
                token_mint = %token_mint,
                signal_source = detection_source.as_str(),
                "payment detector failed while resolving invoice reference match across invoices"
            );
            return Err(error);
        }
    };

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
                &transfer.account_keys,
                shared,
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
    .await;
    let payment_result = match payment_result {
        Ok(payment_result) => payment_result,
        Err(error) => {
            shared.metrics.rpc_failures.fetch_add(1, Ordering::Relaxed);
            shared.release_signature(&signature.signature).await;
            tracing::error!(
                error = %error,
                tx_signature = %signature.signature,
                invoice_id = %invoice.id,
                recipient_token_account = %recipient_token_account,
                token_mint = %token_mint,
                signal_source = detection_source.as_str(),
                "payment detector failed while recording confirmed payment"
            );
            return Err(error);
        }
    };

    if !payment_result.inserted {
        shared
            .metrics
            .duplicate_detection_attempts
            .fetch_add(1, Ordering::Relaxed);
        tracing::info!(
            tx_signature = %signature.signature,
            signal_source = detection_source.as_str(),
            result = "ignored",
            reason = "already_processed",
            "detector ignored transaction"
        );
        return Ok(());
    }

    shared
        .metrics
        .matched_payments
        .fetch_add(1, Ordering::Relaxed);
    if let Some(detection_latency_ms) = detection_latency_ms {
        shared
            .metrics
            .detection_latency_ms_total
            .fetch_add(detection_latency_ms, Ordering::Relaxed);
        shared
            .metrics
            .detection_latency_samples
            .fetch_add(1, Ordering::Relaxed);
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
    account_keys: &[String],
    shared: &DetectorShared,
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
            linked_invoice_id: matched_invoice_id,
            metadata: Some(serde_json::json!({
                "invoice_status": invoice_status,
                "expected_amount_usdc": expected_amount_usdc.map(|amount| amount.normalize().to_string()),
                "detection_source": detection_source.as_str(),
                "account_keys": account_keys,
            })),
        },
    )
    .await;
    let inserted = match inserted {
        Ok(inserted) => inserted,
        Err(error) => {
            shared.release_signature(tx_signature).await;
            return Err(error);
        }
    };
    shared
        .metrics
        .unmatched_payments
        .fetch_add(1, Ordering::Relaxed);

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

fn target_priority_cmp(
    left: &invoices::PendingSettlementTarget,
    right: &invoices::PendingSettlementTarget,
) -> std::cmp::Ordering {
    right
        .newest_invoice_created_at
        .cmp(&left.newest_invoice_created_at)
        .then_with(|| right.open_invoice_count.cmp(&left.open_invoice_count))
        .then_with(|| left.usdc_ata.cmp(&right.usdc_ata))
}

fn target_age(target: &invoices::PendingSettlementTarget) -> Duration {
    let age = Utc::now().signed_duration_since(target.newest_invoice_created_at);
    age.to_std().unwrap_or_default()
}

fn target_base_poll_interval(
    config: &PaymentDetectorConfig,
    target: &invoices::PendingSettlementTarget,
    has_active_logs_target: bool,
) -> Duration {
    let age = target_age(target);
    let interval = if age <= config.fast_window {
        config.fast_poll_interval
    } else if age <= config.medium_window {
        config.medium_poll_interval
    } else {
        config.slow_poll_interval
    };

    if has_active_logs_target {
        interval.max(config.medium_poll_interval)
    } else {
        interval
    }
}

fn next_poll_delay(
    config: &PaymentDetectorConfig,
    target: &invoices::PendingSettlementTarget,
    idle_poll_streak: u32,
    has_active_logs_target: bool,
) -> Duration {
    let base = target_base_poll_interval(config, target, has_active_logs_target);
    let multiplier = 1u32 << idle_poll_streak.min(4);
    base.checked_mul(multiplier)
        .unwrap_or(config.max_idle_backoff)
        .min(config.max_idle_backoff)
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
    let message: LogsWebsocketMessage = serde_json::from_str(payload)
        .map_err(|error| AppError::Internal(anyhow::Error::new(error)))?;

    Ok(match message {
        LogsWebsocketMessage::SubscribeAck {
            id: Some(1),
            result,
        } => Some(LogsSubscribeAck {
            subscription_id: result,
        }),
        LogsWebsocketMessage::SubscribeAck { .. }
        | LogsWebsocketMessage::Notification { .. }
        | LogsWebsocketMessage::Unknown => None,
    })
}

fn parse_logs_notification(payload: &str) -> AppResult<Option<LogsNotification>> {
    let message: LogsWebsocketMessage = serde_json::from_str(payload)
        .map_err(|error| AppError::Internal(anyhow::Error::new(error)))?;

    Ok(match message {
        LogsWebsocketMessage::Notification { method, params } if method == "logsNotification" => {
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
        .filter(
            |target| match UsdcSettlement::from_wallet_pubkey(&target.wallet_pubkey) {
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
            },
        )
        .collect()
}

async fn load_pending_targets(
    pool: &PgPool,
    pending_invoice_ttl: Duration,
) -> Vec<invoices::PendingSettlementTarget> {
    if let Err(error) = maintain_pending_invoices(pool, pending_invoice_ttl).await {
        tracing::warn!(error = %error, "[detector] failed to maintain pending invoices");
    }

    match invoices::list_pending_settlement_targets(pool).await {
        Ok(targets) => sanitize_pending_targets(targets),
        Err(error) => {
            tracing::warn!(error = %error, "[detector] failed to load pending settlement targets");
            Vec::new()
        }
    }
}

async fn maintain_pending_invoices(pool: &PgPool, pending_invoice_ttl: Duration) -> AppResult<()> {
    let expired_stale = invoices::expire_pending_older_than(pool, pending_invoice_ttl).await?;
    if expired_stale > 0 {
        tracing::info!(
            expired_stale,
            pending_invoice_ttl_secs = pending_invoice_ttl.as_secs(),
            "[detector] expired stale pending invoices"
        );
    }

    let expired_invalid = invoices::expire_invalid_pending_destinations(pool).await?;
    if expired_invalid > 0 {
        tracing::info!(
            expired_invalid,
            "[detector] expired invalid pending invoice destinations"
        );
    }

    Ok(())
}

fn format_target_preview(targets: &[invoices::PendingSettlementTarget]) -> String {
    if targets.is_empty() {
        return "none".to_string();
    }

    const MAX_TARGET_PREVIEW: usize = 3;
    let mut preview = targets
        .iter()
        .take(MAX_TARGET_PREVIEW)
        .map(|target| target.usdc_ata.as_str())
        .collect::<Vec<_>>()
        .join(", ");

    if targets.len() > MAX_TARGET_PREVIEW {
        preview.push_str(&format!(", +{} more", targets.len() - MAX_TARGET_PREVIEW));
    }

    preview
}

fn redact_ws_url(value: &str) -> String {
    let Some((base, query)) = value.split_once('?') else {
        return value.to_string();
    };

    let redacted_query = query
        .split('&')
        .map(|pair| match pair.split_once('=') {
            Some((key, _)) if key.eq_ignore_ascii_case("api-key") => {
                format!("{key}=REDACTED")
            }
            _ => pair.to_string(),
        })
        .collect::<Vec<_>>()
        .join("&");

    if value.starts_with("wss://") || value.starts_with("ws://") {
        return format!("{base}?{redacted_query}");
    }

    format!("{base}?{redacted_query}")
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
    use chrono::{Duration as ChronoDuration, Utc};
    use rust_decimal::Decimal;
    use std::time::Duration;
    use uuid::Uuid;

    use super::{
        next_poll_delay, resolve_reference_settlement, target_base_poll_interval,
        PaymentDetectorConfig, ReferenceSettlement, UnmatchedReason,
    };
    use crate::services::invoices::{PendingSettlementTarget, ReferenceMatchCandidate};

    fn detector_config() -> PaymentDetectorConfig {
        PaymentDetectorConfig {
            scheduler_tick: Duration::from_secs(5),
            fast_poll_interval: Duration::from_secs(6),
            medium_poll_interval: Duration::from_secs(20),
            slow_poll_interval: Duration::from_secs(60),
            fast_window: Duration::from_secs(120),
            medium_window: Duration::from_secs(900),
            max_targets_per_cycle: 6,
            max_active_logs_subscriptions: 12,
            max_idle_backoff: Duration::from_secs(300),
            signature_dedupe_ttl: Duration::from_secs(300),
            signature_limit: 25,
            pending_invoice_ttl: Duration::from_secs(1800),
        }
    }

    fn pending_target(age_secs: i64, open_invoice_count: i64) -> PendingSettlementTarget {
        PendingSettlementTarget {
            usdc_ata: Uuid::new_v4().to_string(),
            usdc_mint: Uuid::new_v4().to_string(),
            wallet_pubkey: Uuid::new_v4().to_string(),
            open_invoice_count,
            newest_invoice_created_at: Utc::now() - ChronoDuration::seconds(age_secs),
            oldest_invoice_created_at: Utc::now() - ChronoDuration::seconds(age_secs),
        }
    }

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

    #[test]
    fn fresh_targets_poll_fast_until_logs_take_over() {
        let config = detector_config();
        let target = pending_target(30, 1);

        assert_eq!(
            target_base_poll_interval(&config, &target, false),
            Duration::from_secs(6)
        );
        assert_eq!(
            target_base_poll_interval(&config, &target, true),
            Duration::from_secs(20)
        );
    }

    #[test]
    fn older_targets_back_off_exponentially_and_cap() {
        let config = detector_config();
        let target = pending_target(3_600, 1);

        assert_eq!(
            next_poll_delay(&config, &target, 0, false),
            Duration::from_secs(60)
        );
        assert_eq!(
            next_poll_delay(&config, &target, 1, false),
            Duration::from_secs(120)
        );
        assert_eq!(
            next_poll_delay(&config, &target, 4, false),
            Duration::from_secs(300)
        );
    }
}
