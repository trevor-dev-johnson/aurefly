use std::{str::FromStr, time::Duration};

use base64::{engine::general_purpose::STANDARD, Engine as _};
use reqwest::{header::HeaderMap, Client};
use serde::Deserialize;
use serde_json::json;
use solana_sdk::{
    hash::Hash,
    signature::{Keypair, Signature},
    signer::Signer,
    transaction::Transaction,
};
use spl_associated_token_account::instruction::create_associated_token_account;

use crate::{
    error::{AppError, AppResult},
    solana::{parse_pubkey, token_program_id, UsdcSettlement, MAINNET_USDC_MINT},
    treasury::TreasuryWallet,
};

const TOKEN_ACCOUNT_SIZE_BYTES: u64 = 165;
const ATA_CREATION_FEE_BUFFER_LAMPORTS: u64 = 10_000;
const RPC_HTTP_TIMEOUT_SECS: u64 = 15;
const RPC_MAX_ATTEMPTS: usize = 5;
const RPC_RATE_LIMIT_RETRY_ATTEMPTS: usize = 2;
const RPC_INITIAL_BACKOFF_MILLIS: u64 = 500;
const RPC_MAX_BACKOFF_MILLIS: u64 = 2_000;
const RPC_RATE_LIMIT_INITIAL_BACKOFF_MILLIS: u64 = 5_000;
const RPC_RATE_LIMIT_MAX_BACKOFF_MILLIS: u64 = 30_000;
const RPC_DEFAULT_RETRY_AFTER_SECS: u64 = 30;

#[derive(Clone)]
pub struct SolanaRpcClient {
    http: Client,
    rpc_url: String,
    ws_url: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ResolvedUsdcSettlement {
    pub wallet_pubkey: String,
    pub usdc_ata: String,
    pub usdc_mint: String,
}

impl SolanaRpcClient {
    pub fn new(rpc_url: String) -> Self {
        Self {
            http: Client::builder()
                .timeout(Duration::from_secs(RPC_HTTP_TIMEOUT_SECS))
                .build()
                .unwrap_or_else(|_| Client::new()),
            ws_url: derive_websocket_url(&rpc_url),
            rpc_url,
        }
    }

    pub fn websocket_url(&self) -> Option<&str> {
        self.ws_url.as_deref()
    }

    pub async fn account_exists(&self, address: &str) -> AppResult<bool> {
        parse_pubkey(address, "address")?;

        let payload = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "getAccountInfo",
            "params": [
                address,
                {
                    "encoding": "base64",
                    "commitment": "confirmed"
                }
            ]
        });

        let rpc: RpcEnvelope<RpcValue<Option<serde_json::Value>>> = self.post(payload).await?;
        rpc_error_to_result(rpc.error, "getAccountInfo")?;
        Ok(rpc.result.and_then(|result| result.value).is_some())
    }

    pub async fn resolve_usdc_settlement_target(
        &self,
        payout_address: &str,
    ) -> AppResult<ResolvedUsdcSettlement> {
        let payout_address = payout_address.trim();
        parse_pubkey(payout_address, "payout_address")?;

        let Some(wallet_pubkey) = self
            .get_usdc_token_account_owner(payout_address)
            .await?
        else {
            return Err(AppError::Validation(
                "payout_address must be an existing USDC associated token account (ATA)".to_string(),
            ));
        };

        let derived = UsdcSettlement::from_wallet_pubkey(&wallet_pubkey)?;
        if payout_address != derived.usdc_ata {
            return Err(AppError::Validation(
                "payout_address must be the merchant's USDC associated token account (ATA), not a wallet pubkey or non-ATA token account".to_string(),
            ));
        }

        Ok(ResolvedUsdcSettlement {
            wallet_pubkey,
            usdc_ata: derived.usdc_ata,
            usdc_mint: derived.usdc_mint,
        })
    }

    pub async fn ensure_associated_token_account(
        &self,
        treasury: &TreasuryWallet,
        fee_payer: &Keypair,
    ) -> AppResult<Option<String>> {
        if self.account_exists(&treasury.usdc_ata).await? {
            return Ok(None);
        }

        let recent_blockhash = self.get_latest_blockhash().await?;
        let payer = fee_payer.pubkey();
        let owner = treasury.keypair.pubkey();
        let mint = parse_pubkey(&treasury.usdc_mint, "usdc_mint")?;
        let payer_balance = self.get_balance_lamports(&payer.to_string()).await?;
        let minimum_balance = self
            .get_token_account_rent_exemption_lamports()
            .await?
            .saturating_add(ATA_CREATION_FEE_BUFFER_LAMPORTS);

        if payer_balance < minimum_balance {
            return Err(AppError::Validation(format!(
                "ATA fee payer {payer} needs at least {} SOL ({} lamports) on mainnet to create the treasury USDC ATA but only has {} SOL ({} lamports)",
                lamports_to_sol(minimum_balance),
                minimum_balance,
                lamports_to_sol(payer_balance),
                payer_balance,
            )));
        }

        let token_program_id = token_program_id()?;
        let instruction =
            create_associated_token_account(&payer, &owner, &mint, &token_program_id);
        let transaction = Transaction::new_signed_with_payer(
            &[instruction],
            Some(&payer),
            &[fee_payer],
            recent_blockhash,
        );
        let signature = self.send_transaction(&transaction).await?;
        self.wait_for_signature_finalized(&signature).await?;

        if !self.account_exists(&treasury.usdc_ata).await? {
            return Err(AppError::Internal(anyhow::anyhow!(
                "treasury USDC ATA creation transaction finalized but the ATA still does not exist"
            )));
        }

        Ok(Some(signature.to_string()))
    }

    pub async fn get_balance_lamports(&self, address: &str) -> AppResult<u64> {
        parse_pubkey(address, "address")?;

        let payload = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "getBalance",
            "params": [
                address,
                {
                    "commitment": "finalized"
                }
            ]
        });

        let rpc: RpcEnvelope<RpcValue<u64>> = self.post(payload).await?;
        rpc_error_to_result(rpc.error, "getBalance")?;
        Ok(rpc.result.map(|result| result.value).unwrap_or_default())
    }

    pub async fn get_finalized_signatures_for_address(
        &self,
        address: &str,
        limit: usize,
        until_signature: Option<&str>,
    ) -> AppResult<Vec<SignatureInfo>> {
        self.get_signatures_for_address(address, limit, until_signature, "finalized")
            .await
    }

    pub async fn get_confirmed_signatures_for_address(
        &self,
        address: &str,
        limit: usize,
        until_signature: Option<&str>,
    ) -> AppResult<Vec<SignatureInfo>> {
        self.get_signatures_for_address(address, limit, until_signature, "confirmed")
            .await
    }

    pub async fn get_finalized_usdc_transfer_to_token_account(
        &self,
        signature: &str,
        recipient_token_account: &str,
        mint: &str,
    ) -> AppResult<Option<ParsedUsdcTransfer>> {
        self.get_usdc_transfer_to_token_account(signature, recipient_token_account, mint, "finalized")
            .await
    }

    pub async fn get_confirmed_usdc_transfer_to_token_account(
        &self,
        signature: &str,
        recipient_token_account: &str,
        mint: &str,
    ) -> AppResult<Option<ParsedUsdcTransfer>> {
        self.get_usdc_transfer_to_token_account(signature, recipient_token_account, mint, "confirmed")
            .await
    }

    async fn get_signatures_for_address(
        &self,
        address: &str,
        limit: usize,
        until_signature: Option<&str>,
        commitment: &str,
    ) -> AppResult<Vec<SignatureInfo>> {
        parse_pubkey(address, "address")?;

        let mut config = serde_json::Map::new();
        config.insert("commitment".to_string(), json!(commitment));
        config.insert("limit".to_string(), json!(limit));

        if let Some(signature) = until_signature {
            config.insert("until".to_string(), json!(signature));
        }

        let payload = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "getSignaturesForAddress",
            "params": [
                address,
                config
            ]
        });

        let rpc: RpcEnvelope<Vec<SignatureInfo>> = self.post(payload).await?;
        rpc_error_to_result(rpc.error, "getSignaturesForAddress")?;
        Ok(rpc.result.unwrap_or_default())
    }

    async fn get_usdc_token_account_owner(&self, address: &str) -> AppResult<Option<String>> {
        parse_pubkey(address, "address")?;

        let payload = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "getAccountInfo",
            "params": [
                address,
                {
                    "encoding": "jsonParsed",
                    "commitment": "finalized"
                }
            ]
        });

        let rpc: RpcEnvelope<RpcValue<Option<serde_json::Value>>> = self.post(payload).await?;
        rpc_error_to_result(rpc.error, "getAccountInfo")?;

        let Some(account_info) = rpc.result.and_then(|result| result.value) else {
            return Ok(None);
        };

        let token_program_id = token_program_id()?.to_string();
        let owner_program = account_info
            .get("owner")
            .and_then(|value| value.as_str());
        let parsed_type = account_info
            .get("data")
            .and_then(|value| value.get("parsed"))
            .and_then(|value| value.get("type"))
            .and_then(|value| value.as_str());

        if owner_program != Some(token_program_id.as_str()) || parsed_type != Some("account") {
            return Ok(None);
        }

        let Some(info) = account_info
            .get("data")
            .and_then(|value| value.get("parsed"))
            .and_then(|value| value.get("info"))
        else {
            return Ok(None);
        };

        let Some(mint) = info.get("mint").and_then(|value| value.as_str()) else {
            return Ok(None);
        };
        let Some(owner) = info.get("owner").and_then(|value| value.as_str()) else {
            return Ok(None);
        };

        if mint != MAINNET_USDC_MINT {
            return Ok(None);
        }

        Ok(Some(owner.to_string()))
    }

    async fn get_usdc_transfer_to_token_account(
        &self,
        signature: &str,
        recipient_token_account: &str,
        mint: &str,
        commitment: &str,
    ) -> AppResult<Option<ParsedUsdcTransfer>> {
        let payload = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "getTransaction",
            "params": [
                signature,
                {
                    "commitment": commitment,
                    "encoding": "jsonParsed",
                    "maxSupportedTransactionVersion": 0
                }
            ]
        });

        let rpc: RpcEnvelope<Option<GetTransactionResult>> = self.post(payload).await?;
        rpc_error_to_result(rpc.error, "getTransaction")?;

        let Some(transaction) = rpc.result.flatten() else {
            return Ok(None);
        };
        let Some(meta) = transaction.meta else {
            return Ok(None);
        };

        let amount_usdc = token_account_delta(
            &transaction.transaction.message.account_keys,
            &meta.pre_token_balances,
            &meta.post_token_balances,
            recipient_token_account,
            mint,
        )?;

        if amount_usdc <= rust_decimal::Decimal::ZERO {
            return Ok(None);
        }

        Ok(Some(ParsedUsdcTransfer {
            amount_usdc,
            source_owner: source_owner_for_transfer(
                &transaction.transaction.message.account_keys,
                &meta.pre_token_balances,
                &meta.post_token_balances,
                recipient_token_account,
                mint,
            ),
            finalized_at: transaction
                .block_time
                .and_then(|seconds| chrono::DateTime::<chrono::Utc>::from_timestamp(seconds, 0)),
            account_keys: transaction
                .transaction
                .message
                .account_keys
                .iter()
                .filter_map(parsed_account_key_str)
                .map(|account_key| account_key.to_string())
                .collect(),
        }))
    }

    async fn get_latest_blockhash(&self) -> AppResult<Hash> {
        let payload = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "getLatestBlockhash",
            "params": [
                {
                    "commitment": "finalized"
                }
            ]
        });

        let rpc: RpcEnvelope<RpcValue<LatestBlockhashValue>> = self.post(payload).await?;
        rpc_error_to_result(rpc.error, "getLatestBlockhash")?;
        let blockhash = rpc
            .result
            .ok_or_else(|| AppError::Internal(anyhow::anyhow!("missing latest blockhash result")))?
            .value
            .blockhash;

        Hash::from_str(&blockhash).map_err(|error| AppError::Internal(anyhow::Error::new(error)))
    }

    async fn get_token_account_rent_exemption_lamports(&self) -> AppResult<u64> {
        let payload = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "getMinimumBalanceForRentExemption",
            "params": [TOKEN_ACCOUNT_SIZE_BYTES]
        });

        let rpc: RpcEnvelope<u64> = self.post(payload).await?;
        rpc_error_to_result(rpc.error, "getMinimumBalanceForRentExemption")?;
        rpc.result.ok_or_else(|| {
            AppError::Internal(anyhow::anyhow!(
                "missing getMinimumBalanceForRentExemption result"
            ))
        })
    }

    async fn send_transaction(&self, transaction: &Transaction) -> AppResult<Signature> {
        let bytes = bincode::serialize(transaction)
            .map_err(|error| AppError::Internal(anyhow::Error::new(error)))?;
        let payload = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "sendTransaction",
            "params": [
                STANDARD.encode(bytes),
                {
                    "encoding": "base64",
                    "preflightCommitment": "finalized",
                    "maxRetries": 5
                }
            ]
        });

        let rpc: RpcEnvelope<String> = self.post(payload).await?;
        rpc_error_to_result(rpc.error, "sendTransaction")?;
        let signature = rpc
            .result
            .ok_or_else(|| AppError::Internal(anyhow::anyhow!("missing sendTransaction signature")))?;

        Signature::from_str(&signature).map_err(|error| AppError::Internal(anyhow::Error::new(error)))
    }

    async fn wait_for_signature_finalized(&self, signature: &Signature) -> AppResult<()> {
        let signature = signature.to_string();

        for _ in 0..30 {
            let payload = json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": "getSignatureStatuses",
                "params": [
                    [signature],
                    {
                        "searchTransactionHistory": true
                    }
                ]
            });

            let rpc: RpcEnvelope<SignatureStatusesValue> = self.post(payload).await?;
            rpc_error_to_result(rpc.error, "getSignatureStatuses")?;

            let Some(status) = rpc
                .result
                .and_then(|result| result.value.into_iter().next())
                .flatten()
            else {
                tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                continue;
            };

            if let Some(error) = status.err {
                return Err(AppError::Internal(anyhow::anyhow!(
                    "ATA creation transaction failed: {error}"
                )));
            }

            if status.confirmation_status.as_deref() == Some("finalized") {
                return Ok(());
            }

            tokio::time::sleep(std::time::Duration::from_secs(2)).await;
        }

        Err(AppError::Internal(anyhow::anyhow!(
            "timed out waiting for ATA creation transaction to finalize"
        )))
    }

    async fn post<T: for<'de> Deserialize<'de>>(
        &self,
        payload: serde_json::Value,
    ) -> AppResult<T> {
        let method = payload
            .get("method")
            .and_then(|value| value.as_str())
            .unwrap_or("unknown");
        let mut backoff = Duration::from_millis(RPC_INITIAL_BACKOFF_MILLIS);

        for attempt in 1..=RPC_MAX_ATTEMPTS {
            let response = match self.http.post(&self.rpc_url).json(&payload).send().await {
                Ok(response) => response,
                Err(error) => {
                    if attempt < RPC_MAX_ATTEMPTS {
                        tracing::warn!(
                            rpc_method = method,
                            attempt,
                            max_attempts = RPC_MAX_ATTEMPTS,
                            backoff_ms = backoff.as_millis() as u64,
                            error = %error,
                            "Solana RPC transport failure; retrying"
                        );
                        tokio::time::sleep(backoff).await;
                        backoff = next_backoff(backoff, Duration::from_millis(RPC_MAX_BACKOFF_MILLIS));
                        continue;
                    }

                    return Err(AppError::Internal(anyhow::anyhow!(
                        "Solana RPC {method} failed after {RPC_MAX_ATTEMPTS} attempts: {error}"
                    )));
                }
            };

            let status = response.status();
            if !status.is_success() {
                let retry_after = retry_after_from_headers(response.headers())
                    .unwrap_or_else(|| default_retry_after_for_status(status.as_u16(), attempt));
                let response_body = response.text().await.unwrap_or_default();

                if status.as_u16() == 429 {
                    if attempt < RPC_RATE_LIMIT_RETRY_ATTEMPTS {
                        tracing::warn!(
                            rpc_method = method,
                            attempt,
                            max_attempts = RPC_RATE_LIMIT_RETRY_ATTEMPTS,
                            status = status.as_u16(),
                            backoff_ms = retry_after.as_millis() as u64,
                            response_body = %truncate_for_log(&response_body),
                            "Solana RPC rate limited; retrying with cooldown"
                        );
                        tokio::time::sleep(retry_after).await;
                        continue;
                    }

                    return Err(AppError::RateLimited {
                        service: "Solana RPC",
                        operation: method.to_string(),
                        retry_after_secs: retry_after.as_secs().max(1),
                    });
                }

                if attempt < RPC_MAX_ATTEMPTS && should_retry_http_status(status.as_u16()) {
                    tracing::warn!(
                        rpc_method = method,
                        attempt,
                        max_attempts = RPC_MAX_ATTEMPTS,
                        status = status.as_u16(),
                        backoff_ms = backoff.as_millis() as u64,
                        response_body = %truncate_for_log(&response_body),
                        "Solana RPC HTTP failure; retrying"
                    );
                    tokio::time::sleep(backoff).await;
                    backoff = next_backoff(backoff, Duration::from_millis(RPC_MAX_BACKOFF_MILLIS));
                    continue;
                }

                return Err(AppError::Internal(anyhow::anyhow!(
                    "Solana RPC {method} failed with HTTP {}: {}",
                    status.as_u16(),
                    truncate_for_log(&response_body)
                )));
            }

            match response.json::<T>().await {
                Ok(parsed) => return Ok(parsed),
                Err(error) => {
                    if attempt < RPC_MAX_ATTEMPTS {
                        tracing::warn!(
                            rpc_method = method,
                            attempt,
                            max_attempts = RPC_MAX_ATTEMPTS,
                            backoff_ms = backoff.as_millis() as u64,
                            error = %error,
                            "Solana RPC response decode failure; retrying"
                        );
                        tokio::time::sleep(backoff).await;
                        backoff = next_backoff(backoff, Duration::from_millis(RPC_MAX_BACKOFF_MILLIS));
                        continue;
                    }

                    return Err(AppError::Internal(anyhow::anyhow!(
                        "Solana RPC {method} returned an unreadable response after {RPC_MAX_ATTEMPTS} attempts: {error}"
                    )));
                }
            }
        }

        Err(AppError::Internal(anyhow::anyhow!(
            "Solana RPC {method} exhausted all retry attempts"
        )))
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct SignatureInfo {
    pub signature: String,
    pub err: Option<serde_json::Value>,
    #[serde(rename = "blockTime")]
    pub block_time: Option<i64>,
    #[serde(rename = "confirmationStatus")]
    pub confirmation_status: Option<String>,
    pub slot: i64,
}

#[derive(Debug, Clone)]
pub struct ParsedUsdcTransfer {
    pub amount_usdc: rust_decimal::Decimal,
    pub source_owner: Option<String>,
    pub finalized_at: Option<chrono::DateTime<chrono::Utc>>,
    pub account_keys: Vec<String>,
}

#[derive(Deserialize)]
struct RpcEnvelope<T> {
    result: Option<T>,
    error: Option<RpcError>,
}

#[derive(Deserialize)]
struct RpcValue<T> {
    value: T,
}

#[derive(Deserialize)]
struct RpcError {
    code: Option<i64>,
    message: String,
}

#[derive(Deserialize)]
struct LatestBlockhashValue {
    blockhash: String,
}

#[derive(Deserialize)]
struct SignatureStatusesValue {
    value: Vec<Option<SignatureStatus>>,
}

#[derive(Deserialize)]
struct SignatureStatus {
    err: Option<serde_json::Value>,
    #[serde(rename = "confirmationStatus")]
    confirmation_status: Option<String>,
}

#[derive(Deserialize)]
struct GetTransactionResult {
    #[serde(rename = "blockTime")]
    block_time: Option<i64>,
    transaction: ParsedTransactionEnvelope,
    meta: Option<TransactionMeta>,
}

#[derive(Deserialize)]
struct ParsedTransactionEnvelope {
    message: ParsedMessage,
}

#[derive(Deserialize)]
struct ParsedMessage {
    #[serde(rename = "accountKeys")]
    account_keys: Vec<ParsedAccountKey>,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum ParsedAccountKey {
    Simple(String),
    Parsed { pubkey: String },
}

#[derive(Deserialize)]
struct TransactionMeta {
    #[serde(rename = "preTokenBalances", default)]
    pre_token_balances: Vec<TokenBalance>,
    #[serde(rename = "postTokenBalances", default)]
    post_token_balances: Vec<TokenBalance>,
}

#[derive(Clone, Deserialize)]
struct TokenBalance {
    #[serde(rename = "accountIndex")]
    account_index: usize,
    mint: String,
    owner: Option<String>,
    #[serde(rename = "uiTokenAmount")]
    ui_token_amount: UiTokenAmount,
}

#[derive(Clone, Deserialize)]
struct UiTokenAmount {
    amount: String,
    decimals: u32,
}

fn rpc_error_to_result(error: Option<RpcError>, operation: &str) -> AppResult<()> {
    if let Some(error) = error {
        if error.code == Some(429) {
            return Err(AppError::RateLimited {
                service: "Solana RPC",
                operation: operation.to_string(),
                retry_after_secs: RPC_DEFAULT_RETRY_AFTER_SECS,
            });
        }

        return Err(AppError::Internal(anyhow::anyhow!(
            "solana rpc {operation} error: {}",
            error.message
        )));
    }

    Ok(())
}

fn token_account_delta(
    account_keys: &[ParsedAccountKey],
    pre_balances: &[TokenBalance],
    post_balances: &[TokenBalance],
    recipient_token_account: &str,
    mint: &str,
) -> AppResult<rust_decimal::Decimal> {
    let pre_amount =
        token_amount_for_account(account_keys, pre_balances, recipient_token_account, mint)?
            .unwrap_or(rust_decimal::Decimal::ZERO);
    let post_amount =
        token_amount_for_account(account_keys, post_balances, recipient_token_account, mint)?
            .unwrap_or(rust_decimal::Decimal::ZERO);

    Ok(post_amount - pre_amount)
}

fn token_amount_for_account(
    account_keys: &[ParsedAccountKey],
    balances: &[TokenBalance],
    recipient_token_account: &str,
    mint: &str,
) -> AppResult<Option<rust_decimal::Decimal>> {
    for balance in balances {
        if balance.mint != mint {
            continue;
        }

        let Some(account_key) = account_key_at(account_keys, balance.account_index) else {
            continue;
        };

        if account_key != recipient_token_account {
            continue;
        }

        return Ok(Some(parse_token_amount(
            &balance.ui_token_amount.amount,
            balance.ui_token_amount.decimals,
        )?));
    }

    Ok(None)
}

fn source_owner_for_transfer(
    account_keys: &[ParsedAccountKey],
    pre_balances: &[TokenBalance],
    post_balances: &[TokenBalance],
    recipient_token_account: &str,
    mint: &str,
) -> Option<String> {
    negative_token_deltas(account_keys, pre_balances, post_balances, mint)
        .into_iter()
        .find(|delta| delta.account_key != recipient_token_account)
        .and_then(|delta| delta.owner)
}

fn negative_token_deltas(
    account_keys: &[ParsedAccountKey],
    pre_balances: &[TokenBalance],
    post_balances: &[TokenBalance],
    mint: &str,
) -> Vec<TokenDelta> {
    pre_balances
        .iter()
        .filter(|balance| balance.mint == mint)
        .filter_map(|pre_balance| {
            let account_key = account_key_at(account_keys, pre_balance.account_index)?.to_string();
            let pre_amount = parse_token_amount(
                &pre_balance.ui_token_amount.amount,
                pre_balance.ui_token_amount.decimals,
            )
            .ok()?;
            let post_amount = post_balances
                .iter()
                .find(|post_balance| post_balance.account_index == pre_balance.account_index)
                .and_then(|post_balance| {
                    parse_token_amount(
                        &post_balance.ui_token_amount.amount,
                        post_balance.ui_token_amount.decimals,
                    )
                    .ok()
                })
                .unwrap_or(rust_decimal::Decimal::ZERO);

            if post_amount < pre_amount {
                Some(TokenDelta {
                    account_key,
                    owner: pre_balance.owner.clone(),
                })
            } else {
                None
            }
        })
        .collect()
}

fn parse_token_amount(raw_amount: &str, decimals: u32) -> AppResult<rust_decimal::Decimal> {
    let mut amount = rust_decimal::Decimal::from_str(raw_amount).map_err(|_| {
        AppError::Internal(anyhow::anyhow!(
            "failed to parse token amount returned by Solana RPC"
        ))
    })?;
    amount
        .set_scale(decimals)
        .map_err(|error| AppError::Internal(anyhow::Error::new(error)))?;
    Ok(amount)
}

fn account_key_at(account_keys: &[ParsedAccountKey], index: usize) -> Option<&str> {
    parsed_account_key_str(account_keys.get(index)?)
}

fn parsed_account_key_str(account_key: &ParsedAccountKey) -> Option<&str> {
    match account_key {
        ParsedAccountKey::Simple(pubkey) => Some(pubkey.as_str()),
        ParsedAccountKey::Parsed { pubkey } => Some(pubkey.as_str()),
    }
}

struct TokenDelta {
    account_key: String,
    owner: Option<String>,
}

fn lamports_to_sol(lamports: u64) -> String {
    format!("{:.9}", lamports as f64 / 1_000_000_000_f64)
}

fn next_backoff(current: Duration, max: Duration) -> Duration {
    let doubled_millis = current.as_millis().saturating_mul(2);
    let capped_millis = doubled_millis.min(max.as_millis());
    Duration::from_millis(capped_millis as u64)
}

fn should_retry_http_status(status: u16) -> bool {
    status >= 500
}

fn default_retry_after_for_status(status: u16, attempt: usize) -> Duration {
    if status == 429 {
        let base = Duration::from_millis(RPC_RATE_LIMIT_INITIAL_BACKOFF_MILLIS);
        let max = Duration::from_millis(RPC_RATE_LIMIT_MAX_BACKOFF_MILLIS);
        return rate_limit_backoff_for_attempt(base, max, attempt);
    }

    Duration::from_millis(RPC_INITIAL_BACKOFF_MILLIS)
}

fn rate_limit_backoff_for_attempt(base: Duration, max: Duration, attempt: usize) -> Duration {
    let exponent = attempt.saturating_sub(1).min(4) as u32;
    let multiplier = 1u32 << exponent;
    let backoff = base.checked_mul(multiplier).unwrap_or(max);
    backoff.min(max)
}

fn retry_after_from_headers(headers: &HeaderMap) -> Option<Duration> {
    let seconds = headers
        .get("retry-after")?
        .to_str()
        .ok()?
        .trim()
        .parse::<u64>()
        .ok()?;

    Some(Duration::from_secs(seconds.max(1)))
}

fn truncate_for_log(value: &str) -> String {
    const MAX_LEN: usize = 240;

    if value.chars().count() <= MAX_LEN {
        value.to_string()
    } else {
        let truncated: String = value.chars().take(MAX_LEN).collect();
        format!("{truncated}...")
    }
}

fn derive_websocket_url(rpc_url: &str) -> Option<String> {
    if rpc_url.starts_with("wss://") || rpc_url.starts_with("ws://") {
        return Some(rpc_url.to_string());
    }

    rpc_url
        .strip_prefix("https://")
        .map(|rest| format!("wss://{rest}"))
        .or_else(|| rpc_url.strip_prefix("http://").map(|rest| format!("ws://{rest}")))
}
