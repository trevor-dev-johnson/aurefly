const invoiceCard = document.getElementById("invoice-card");
const invoiceTotal = document.getElementById("invoice-total");
const invoiceSubtotal = document.getElementById("invoice-subtotal");
const invoiceFee = document.getElementById("invoice-fee");
const invoiceDescription = document.getElementById("invoice-description");
const invoiceActionsPanel = document.getElementById("invoice-actions-panel");
const address = document.getElementById("invoice-address");
const qr = document.getElementById("invoice-qr");
const payButton = document.getElementById("pay-button");
const copyAddressButton = document.getElementById("copy-address");
const statePanel = document.getElementById("invoice-state");
const stateLabel = document.getElementById("invoice-state-label");
const statusSpinner = document.getElementById("status-spinner");
const statusText = document.getElementById("invoice-status");
const statusDetail = document.getElementById("invoice-status-detail");
const invoiceTxLink = document.getElementById("invoice-tx-link");

const invoiceId = window.location.pathname.split("/").filter(Boolean).pop();
const DEFAULT_POLL_INTERVAL_MS = 10000;
const FAST_POLL_INTERVAL_MS = 1500;
const FAST_POLL_EXTENSION_MS = 60000;
const RETURN_FROM_WALLET_POLL_MS = 30000;

let currentInvoice = null;
let pollTimer = null;
let awaitingWalletApproval = false;
let fastPollUntil = Date.now() + FAST_POLL_EXTENSION_MS;
let copyResetTimer = null;

payButton.addEventListener("click", (event) => {
  if (!currentInvoice || currentInvoice.status === "paid") {
    return;
  }

  event.preventDefault();
  awaitingWalletApproval = true;
  extendFastPolling(FAST_POLL_EXTENSION_MS);
  statusText.textContent = "Open your wallet to approve the payment.";
  scheduleNextPoll(true);
  window.location.assign(currentInvoice.payment_uri);
});

copyAddressButton.addEventListener("click", async () => {
  if (!currentInvoice) {
    return;
  }

  await navigator.clipboard.writeText(currentInvoice.usdc_ata);
  copyAddressButton.textContent = `Copied \u2713 ${addressTail(currentInvoice.usdc_ata)}`;

  if (copyResetTimer) {
    window.clearTimeout(copyResetTimer);
  }

  copyResetTimer = window.setTimeout(() => {
    copyAddressButton.textContent = currentInvoice && currentInvoice.status === "paid" ? "Copied" : "Copy Address";
  }, 1800);
});

document.addEventListener("visibilitychange", () => {
  if (document.hidden || !currentInvoice || currentInvoice.status === "paid") {
    return;
  }

  extendFastPolling(RETURN_FROM_WALLET_POLL_MS);
  scheduleNextPoll(true);
});

async function loadInvoice() {
  const response = await fetch(`/api/v1/invoices/${invoiceId}?observe_payment=true`);
  const invoice = await response.json();

  if (!response.ok) {
    throw new Error(invoice.error || "Unable to load invoice");
  }

  if (invoice.payment_observed && invoice.status !== "paid") {
    awaitingWalletApproval = false;
    extendFastPolling(30000);
  }

  if (invoice.status === "paid") {
    awaitingWalletApproval = false;
  }

  currentInvoice = invoice;
  renderInvoice(invoice);
}

function renderInvoice(invoice) {
  const subtotalAmount = Number(invoice.subtotal_usdc || 0);
  const feeAmount = Number(invoice.platform_fee_usdc || 0);
  const totalAmount = Number(invoice.amount_usdc || 0);
  const paidAmount = Number(invoice.paid_amount_usdc || 0);
  const hasDetectedPayment = paidAmount > 0 && invoice.status !== "paid";
  const hasObservedPayment = Boolean(invoice.payment_observed) && invoice.status !== "paid" && !hasDetectedPayment;
  const txUrl = invoice.latest_payment_tx_url || invoice.payment_observed_tx_url;
  const variant = invoice.status === "paid" ? "paid" : hasDetectedPayment ? "detected" : hasObservedPayment ? "confirming" : "waiting";

  document.title = invoice.status === "paid" ? "Aurefly Receipt" : "Aurefly Invoice";
  invoiceCard.classList.toggle("invoice-card-paid", invoice.status === "paid");
  invoiceActionsPanel.classList.toggle("hidden", invoice.status === "paid");
  invoiceTotal.textContent = formatMoney(totalAmount);
  invoiceSubtotal.textContent = formatMoney(subtotalAmount);
  invoiceFee.textContent = formatMoney(feeAmount);
  invoiceDescription.textContent = invoice.description || "";
  invoiceDescription.classList.toggle("hidden", !invoice.description);
  address.textContent = formatAddress(invoice.usdc_ata);
  qr.src = `/api/v1/public/invoices/${invoice.id}/qr.svg`;
  qr.hidden = false;
  payButton.href = invoice.payment_uri;

  if (!copyResetTimer) {
    copyAddressButton.textContent = "Copy Address";
  }

  statePanel.classList.remove(
    "status-panel-waiting",
    "status-panel-confirming",
    "status-panel-detected",
    "status-panel-paid"
  );
  statePanel.classList.add(`status-panel-${variant}`);

  stateLabel.textContent =
    invoice.status === "paid"
      ? "Payment complete"
      : hasDetectedPayment
        ? "Payment detected..."
        : hasObservedPayment
          ? "Transaction detected... confirming"
          : "Waiting for payment...";

  statusSpinner.classList.toggle("hidden", invoice.status === "paid");

  statusText.textContent =
    invoice.status === "paid"
      ? `${formatMoney(paidAmount)} received.`
      : hasDetectedPayment
        ? `${formatMoney(paidAmount)} received so far. Waiting for the full amount.`
        : hasObservedPayment
          ? "Transaction seen on Solana. Waiting for finalized confirmation."
          : awaitingWalletApproval
            ? "Open your wallet to approve the payment."
            : "Use the button or QR to keep payment routing correct.";

  statusDetail.classList.toggle("hidden", invoice.status !== "paid");
  statusDetail.textContent = invoice.status === "paid" ? "Transaction confirmed on Solana." : "";

  if (txUrl) {
    invoiceTxLink.href = txUrl;
    invoiceTxLink.textContent = invoice.status === "paid" ? "View on Explorer" : "View while confirming";
    invoiceTxLink.classList.remove("hidden");
  } else {
    invoiceTxLink.classList.add("hidden");
  }
}

async function bootstrap() {
  try {
    await loadInvoice();
  } catch (error) {
    invoiceCard.classList.remove("invoice-card-paid");
    invoiceActionsPanel.classList.add("hidden");
    statusSpinner.classList.add("hidden");
    stateLabel.textContent = "Invoice unavailable";
    statusText.textContent = error.message;
  }

  scheduleNextPoll();
}

function scheduleNextPoll(immediate = false) {
  if (pollTimer) {
    window.clearTimeout(pollTimer);
  }

  if (currentInvoice && currentInvoice.status === "paid") {
    pollTimer = null;
    return;
  }

  const delay = immediate ? 0 : nextPollDelayMs();
  pollTimer = window.setTimeout(async () => {
    try {
      await loadInvoice();
    } catch (_) {
      // Keep polling through transient failures.
    }

    scheduleNextPoll();
  }, delay);
}

function nextPollDelayMs() {
  if (document.hidden) {
    return DEFAULT_POLL_INTERVAL_MS;
  }

  return Date.now() < fastPollUntil ? FAST_POLL_INTERVAL_MS : DEFAULT_POLL_INTERVAL_MS;
}

function extendFastPolling(durationMs) {
  fastPollUntil = Math.max(fastPollUntil, Date.now() + durationMs);
}

function formatMoney(value) {
  return new Intl.NumberFormat("en-US", {
    style: "currency",
    currency: "USD",
    minimumFractionDigits: 2,
    maximumFractionDigits: 6,
  }).format(Number(value || 0));
}

function formatAddress(value) {
  if (!value) {
    return "-";
  }

  return `${value.slice(0, 4)}...${addressTail(value)}`;
}

function addressTail(value) {
  return value.slice(-5);
}

bootstrap();
