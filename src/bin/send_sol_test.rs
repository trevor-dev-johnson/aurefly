use std::{env, str::FromStr, time::Duration};

use anyhow::{bail, Context, Result};
use base64::{engine::general_purpose::STANDARD, Engine as _};
use reqwest::Client;
use serde::Deserialize;
use serde_json::json;
use solana_sdk::{
    hash::Hash,
    pubkey::Pubkey,
    signature::{read_keypair_file, Signature},
    signer::Signer,
    system_instruction,
    transaction::Transaction,
};

const HTTP_TIMEOUT_SECS: u64 = 15;

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::from_env()?;
    let http = Client::builder()
        .timeout(Duration::from_secs(HTTP_TIMEOUT_SECS))
        .build()
        .unwrap_or_else(|_| Client::new());
    let payer = read_keypair_file(&args.keypair_path).map_err(|error| {
        anyhow::anyhow!("failed to read keypair from {}: {}", args.keypair_path, error)
    })?;
    let recipient =
        Pubkey::from_str(&args.recipient).context("recipient must be a valid Solana pubkey")?;
    let recent_blockhash = get_latest_blockhash(&http, &args.rpc_url).await?;

    let transaction = Transaction::new_signed_with_payer(
        &[system_instruction::transfer(
            &payer.pubkey(),
            &recipient,
            args.lamports,
        )],
        Some(&payer.pubkey()),
        &[&payer],
        recent_blockhash,
    );

    let signature = send_transaction(&http, &args.rpc_url, &transaction).await?;
    wait_for_signature_finalized(&http, &args.rpc_url, &signature).await?;

    println!("{signature}");
    Ok(())
}

struct Args {
    rpc_url: String,
    keypair_path: String,
    recipient: String,
    lamports: u64,
}

impl Args {
    fn from_env() -> Result<Self> {
        let mut args = env::args().skip(1);
        let rpc_url = args.next().context("usage: send_sol_test <rpc_url> <keypair_path> <recipient> <lamports>")?;
        let keypair_path = args.next().context("missing keypair_path")?;
        let recipient = args.next().context("missing recipient")?;
        let lamports = args
            .next()
            .context("missing lamports")?
            .parse()
            .context("lamports must be a valid u64")?;

        if args.next().is_some() {
            bail!("usage: send_sol_test <rpc_url> <keypair_path> <recipient> <lamports>");
        }

        Ok(Self {
            rpc_url,
            keypair_path,
            recipient,
            lamports,
        })
    }
}

async fn get_latest_blockhash(http: &Client, rpc_url: &str) -> Result<Hash> {
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

    let rpc: RpcEnvelope<RpcValue<LatestBlockhashValue>> = post(http, rpc_url, payload).await?;
    rpc_error_to_result(rpc.error, "getLatestBlockhash")?;
    let blockhash = rpc
        .result
        .context("missing latest blockhash result")?
        .value
        .blockhash;

    Hash::from_str(&blockhash).context("failed to parse latest blockhash")
}

async fn send_transaction(http: &Client, rpc_url: &str, transaction: &Transaction) -> Result<Signature> {
    let bytes = bincode::serialize(transaction).context("failed to serialize transaction")?;
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

    let rpc: RpcEnvelope<String> = post(http, rpc_url, payload).await?;
    rpc_error_to_result(rpc.error, "sendTransaction")?;
    let signature = rpc.result.context("missing sendTransaction signature")?;
    Signature::from_str(&signature).context("failed to parse sendTransaction signature")
}

async fn wait_for_signature_finalized(http: &Client, rpc_url: &str, signature: &Signature) -> Result<()> {
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

        let rpc: RpcEnvelope<SignatureStatusesValue> = post(http, rpc_url, payload).await?;
        rpc_error_to_result(rpc.error, "getSignatureStatuses")?;

        let Some(status) = rpc
            .result
            .and_then(|value| value.value.into_iter().next())
            .flatten()
        else {
            tokio::time::sleep(Duration::from_secs(2)).await;
            continue;
        };

        if let Some(error) = status.err {
            bail!("SOL test transfer failed: {error}");
        }

        if status.confirmation_status.as_deref() == Some("finalized") {
            return Ok(());
        }

        tokio::time::sleep(Duration::from_secs(2)).await;
    }

    bail!("timed out waiting for SOL test transfer to finalize")
}

async fn post<T: for<'de> Deserialize<'de>>(
    http: &Client,
    rpc_url: &str,
    payload: serde_json::Value,
) -> Result<T> {
    let response = http
        .post(rpc_url)
        .json(&payload)
        .send()
        .await
        .with_context(|| format!("failed to send RPC request to {rpc_url}"))?;
    let status = response.status();
    let body = response.text().await.context("failed to read RPC response body")?;

    if !status.is_success() {
        bail!("RPC HTTP {}: {}", status.as_u16(), body);
    }

    serde_json::from_str(&body).context("failed to decode RPC response")
}

fn rpc_error_to_result(error: Option<RpcError>, operation: &str) -> Result<()> {
    if let Some(error) = error {
        bail!("rpc {operation} error: {}", error.message);
    }

    Ok(())
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
