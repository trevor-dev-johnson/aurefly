const fs = require("fs");
const path = require("path");
const { Keypair, PublicKey, Connection, Transaction, TransactionInstruction } = require("@solana/web3.js");

const API_BASE = process.argv[2] || "http://localhost:8080/api/v1";
const APP_BASE = process.argv[3] || "http://localhost:3000";
const SUBTOTAL_USDC = process.argv[4] || "0.020";
const PAYOUT_ADDRESS = process.argv[5] || process.env.PAYOUT_ADDRESS || "AbC2BEBTyK45VHyeFodk7HBmeTzJBUoBxAvbt8nTXEUy";
const KEYPAIR_PATH =
  process.argv[6] || path.join(process.env.USERPROFILE || process.env.HOME || "", ".config", "solana", "id.json");
const USDC_DECIMALS = 6;
const TOKEN_PROGRAM_ID = new PublicKey("TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA");
const ASSOCIATED_TOKEN_PROGRAM_ID = new PublicKey("ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL");
const WAIT_TIMEOUT_MS = 120000;
const WAIT_INTERVAL_MS = 2000;
const PASSWORD = "pay-invoice-from-uri-password";

async function main() {
  const rpcUrl = resolveRpcUrl(path.join(process.cwd(), ".env"));
  const rpcProvider = detectRpcProvider(rpcUrl);
  const payer = loadKeypair(KEYPAIR_PATH);
  const connection = new Connection(rpcUrl, "finalized");
  const email = `uri-${Date.now()}@example.com`;

  const auth = await requestJson("POST", `${API_BASE}/auth/sign-up`, {
    email,
    password: PASSWORD,
  });
  const signIn = await requestJson("POST", `${API_BASE}/auth/sign-in`, {
    email,
    password: PASSWORD,
  });
  const invoice = await requestJsonWithToken(
    "POST",
    `${API_BASE}/me/invoices`,
    {
      amount_usdc: SUBTOTAL_USDC,
      payout_address: PAYOUT_ADDRESS,
      description: "URI payment smoke",
    },
    signIn.token
  );
  const payPageUrl = `${APP_BASE}/pay/${invoice.id}`;
  const payPageResponse = await fetch(payPageUrl);
  const publicInvoice = await requestJson("GET", `${API_BASE}/public/invoices/${invoice.id}`);
  if (!invoice.reference_pubkey || !invoice.payment_uri.includes(`&reference=${invoice.reference_pubkey}`)) {
    throw new Error(`invoice payment URI is missing the required reference: ${invoice.payment_uri}`);
  }
  if (publicInvoice.payment_uri !== invoice.payment_uri) {
    throw new Error("public invoice payment URI does not match the authenticated invoice URI");
  }
  if (payPageResponse.status !== 200) {
    throw new Error(`pay page returned ${payPageResponse.status}`);
  }
  const payment = await payInvoiceFromUri(connection, payer, invoice.payment_uri);
  const observedInvoice = await requestJson("GET", `${API_BASE}/public/invoices/${invoice.id}`);
  const paidInvoice = await waitForInvoicePaid(invoice.id);
  const dashboardInvoices = await requestJsonWithToken("GET", `${API_BASE}/me/invoices`, undefined, signIn.token);
  const dashboardInvoice = dashboardInvoices.find((item) => item.id === invoice.id) || null;

  const summary = {
    rpc_provider: rpcProvider,
    rpc_url: redactRpcUrl(rpcUrl),
    pay_page_url: payPageUrl,
    pay_page_status: payPageResponse.status,
    invoice_id: invoice.id,
    invoice_reference_pubkey: invoice.reference_pubkey,
    public_invoice_reference_pubkey: publicInvoice.reference_pubkey,
    invoice_amount_usdc: invoice.amount_usdc,
    payment_uri_has_exact_reference: invoice.payment_uri.includes(`&reference=${invoice.reference_pubkey}`),
    public_payment_uri_matches_private: publicInvoice.payment_uri === invoice.payment_uri,
    payment_recipient_wallet: payment.recipientWallet,
    payment_recipient_ata: payment.recipientAta,
    tx_signature: payment.signature,
    finalized_in_secs: payment.finalizedInSecs,
    observed_status_before_paid: observedInvoice.status,
    payment_observed_before_paid: observedInvoice.payment_observed,
    payment_observed_tx_signature: observedInvoice.payment_observed_tx_signature,
    invoice_paid_in_secs: paidInvoice.detectedInSecs,
    latest_payment_tx_signature: paidInvoice.invoice.latest_payment_tx_signature,
    invoice_status: paidInvoice.invoice.status,
    invoice_present_in_dashboard: Boolean(dashboardInvoice),
    dashboard_invoice_status: dashboardInvoice?.status ?? null,
    dashboard_latest_payment_tx_signature: dashboardInvoice?.latest_payment_tx_signature ?? null,
  };

  process.stdout.write(`${JSON.stringify(summary, null, 2)}\n`);
}

async function payInvoiceFromUri(connection, payer, paymentUri) {
  const parsed = parsePaymentUri(paymentUri);
  const mint = new PublicKey(parsed.mint);
  const recipientWallet = new PublicKey(parsed.recipient);
  const destination = deriveAta(recipientWallet, mint);
  const references = parsed.references.map((value) => new PublicKey(value));
  const owner = payer.publicKey;
  const source = deriveAta(owner, mint);
  const amount = decimalToBaseUnits(parsed.amount, USDC_DECIMALS);

  const sourceBalance = await connection.getTokenAccountBalance(source, "finalized").catch(() => null);
  if (!sourceBalance) {
    throw new Error(`source token account ${source.toBase58()} does not exist`);
  }

  const available = BigInt(sourceBalance.value.amount);
  if (available < amount) {
    throw new Error(
      `insufficient USDC balance in ${source.toBase58()}: have ${available.toString()} base units, need ${amount.toString()}`
    );
  }

  const instruction = createTransferCheckedWithReferencesInstruction({
    source,
    mint,
    destination,
    owner,
    amount,
    decimals: USDC_DECIMALS,
    references,
  });

  const latestBlockhash = await connection.getLatestBlockhash("finalized");
  const transaction = new Transaction({
    feePayer: owner,
    blockhash: latestBlockhash.blockhash,
    lastValidBlockHeight: latestBlockhash.lastValidBlockHeight,
  }).add(instruction);
  transaction.sign(payer);

  const startedAt = Date.now();
  const signature = await connection.sendRawTransaction(transaction.serialize(), {
    preflightCommitment: "confirmed",
  });
  await connection.confirmTransaction(
    {
      signature,
      blockhash: latestBlockhash.blockhash,
      lastValidBlockHeight: latestBlockhash.lastValidBlockHeight,
    },
    "finalized"
  );

  return {
    signature,
    recipientWallet: recipientWallet.toBase58(),
    recipientAta: destination.toBase58(),
    finalizedInSecs: roundSeconds(Date.now() - startedAt),
  };
}

async function waitForInvoicePaid(invoiceId) {
  const startedAt = Date.now();
  const deadline = startedAt + WAIT_TIMEOUT_MS;

  while (Date.now() < deadline) {
    const invoice = await requestJson("GET", `${API_BASE}/public/invoices/${invoiceId}`);
    if (invoice.status === "paid") {
      return {
        invoice,
        detectedInSecs: roundSeconds(Date.now() - startedAt),
      };
    }

    await sleep(WAIT_INTERVAL_MS);
  }

  throw new Error(`invoice ${invoiceId} did not become paid within ${WAIT_TIMEOUT_MS / 1000}s`);
}

function createTransferCheckedWithReferencesInstruction({
  source,
  mint,
  destination,
  owner,
  amount,
  decimals,
  references,
}) {
  const data = Buffer.alloc(10);
  data.writeUInt8(12, 0);
  data.writeBigUInt64LE(amount, 1);
  data.writeUInt8(decimals, 9);

  return new TransactionInstruction({
    programId: TOKEN_PROGRAM_ID,
    keys: [
      { pubkey: source, isSigner: false, isWritable: true },
      { pubkey: mint, isSigner: false, isWritable: false },
      { pubkey: destination, isSigner: false, isWritable: true },
      { pubkey: owner, isSigner: true, isWritable: false },
      ...references.map((pubkey) => ({
        pubkey,
        isSigner: false,
        isWritable: false,
      })),
    ],
    data,
  });
}

function deriveAta(owner, mint) {
  return PublicKey.findProgramAddressSync(
    [owner.toBuffer(), TOKEN_PROGRAM_ID.toBuffer(), mint.toBuffer()],
    ASSOCIATED_TOKEN_PROGRAM_ID
  )[0];
}

function parsePaymentUri(paymentUri) {
  const [recipientPart, queryPart = ""] = paymentUri.split("?");
  const recipient = recipientPart.replace(/^solana:/, "");
  const params = new URLSearchParams(queryPart);
  const amount = params.get("amount");
  const mint = params.get("spl-token");
  const references = params.getAll("reference");

  if (!recipient || !amount || !mint || references.length === 0) {
    throw new Error(`unexpected Solana Pay URI: ${paymentUri}`);
  }

  return {
    recipient,
    amount,
    mint,
    references,
  };
}

function decimalToBaseUnits(value, decimals) {
  const [whole, fractional = ""] = value.split(".");
  if (fractional.length > decimals) {
    throw new Error(`amount ${value} exceeds ${decimals} decimal places`);
  }

  const normalized = `${whole}${fractional.padEnd(decimals, "0")}`;
  return BigInt(normalized);
}

function resolveRpcUrl(envPath) {
  const env = parseEnv(envPath);
  if (env.SOLANA_RPC_URL) {
    return env.SOLANA_RPC_URL;
  }
  if (env.HELIUS_API_KEY) {
    return `https://mainnet.helius-rpc.com/?api-key=${env.HELIUS_API_KEY}`;
  }
  return "https://api.mainnet-beta.solana.com";
}

function parseEnv(envPath) {
  const result = {};
  const content = fs.readFileSync(envPath, "utf8");

  for (const line of content.split(/\r?\n/)) {
    const trimmed = line.trim();
    if (!trimmed || trimmed.startsWith("#")) {
      continue;
    }

    const separatorIndex = trimmed.indexOf("=");
    if (separatorIndex === -1) {
      continue;
    }

    const key = trimmed.slice(0, separatorIndex).trim();
    const value = trimmed.slice(separatorIndex + 1).trim();
    result[key] = value;
  }

  return result;
}

function loadKeypair(filePath) {
  const secret = JSON.parse(fs.readFileSync(filePath, "utf8"));
  return Keypair.fromSecretKey(Uint8Array.from(secret));
}

function detectRpcProvider(rpcUrl) {
  if (rpcUrl.includes("helius")) {
    return "helius";
  }
  if (rpcUrl.includes("quicknode")) {
    return "quicknode";
  }
  if (rpcUrl.includes("triton")) {
    return "triton";
  }
  if (rpcUrl.includes("solana.com")) {
    return "solana_public";
  }
  return "custom";
}

function redactRpcUrl(value) {
  const [base, query] = value.split("?");
  if (!query) {
    return value;
  }

  const params = new URLSearchParams(query);
  if (params.has("api-key")) {
    params.set("api-key", "REDACTED");
  }

  return `${base}?${params.toString()}`;
}

function roundSeconds(milliseconds) {
  return Number((milliseconds / 1000).toFixed(2));
}

function sleep(milliseconds) {
  return new Promise((resolve) => setTimeout(resolve, milliseconds));
}

async function requestJson(method, url, body) {
  return requestJsonWithToken(method, url, body, null);
}

async function requestJsonWithToken(method, url, body, token) {
  const headers = body ? { "Content-Type": "application/json" } : {};
  if (token) {
    headers.Authorization = `Bearer ${token}`;
  }

  const response = await fetch(url, {
    method,
    headers,
    body: body ? JSON.stringify(body) : undefined,
  });

  const payload = await response.json();
  if (!response.ok) {
    throw new Error(payload.error || `${method} ${url} failed`);
  }

  return payload;
}

main().catch((error) => {
  process.stderr.write(`${error.stack || error.message}\n`);
  process.exit(1);
});
