const API_BASE = "/api/v1";
const TOKEN_KEY = "aurefly_auth_token";
const PLATFORM_FEE_RATE = 0;
const DEMO_INVOICE_ID = "544ff0a7-3ee1-4d42-aa74-2305dc6921bf";
const DEMO_INVOICE_FALLBACK_URL = `https://aurefly.com/pay/${DEMO_INVOICE_ID}`;

const landingScreen = document.getElementById("landing-screen");
const authScreen = document.getElementById("auth-screen");
const dashboardScreen = document.getElementById("dashboard-screen");
const landingStatus = document.getElementById("landing-status");
const getStartedButton = document.getElementById("get-started");
const viewDemoButton = document.getElementById("view-demo");
const authForm = document.getElementById("auth-form");
const authToggle = document.getElementById("auth-toggle");
const authSwitchLabel = document.getElementById("auth-switch-label");
const authSubmit = document.getElementById("auth-submit");
const authStatus = document.getElementById("auth-status");
const backToLandingButton = document.getElementById("back-to-landing");
const emailInput = document.getElementById("email");
const passwordInput = document.getElementById("password");
const dashboardSubtitle = document.getElementById("dashboard-subtitle");
const totalReceived = document.getElementById("total-received");
const invoiceList = document.getElementById("invoice-list");
const invoiceStatus = document.getElementById("invoice-status");
const refreshButton = document.getElementById("refresh-button");
const logoutButton = document.getElementById("logout-button");
const invoiceModal = document.getElementById("invoice-modal");
const openInvoiceModalButton = document.getElementById("open-invoice-modal");
const closeInvoiceModalButton = document.getElementById("close-invoice-modal");
const invoiceForm = document.getElementById("invoice-form");
const invoiceAmountInput = document.getElementById("invoice-amount");
const invoiceDescriptionInput = document.getElementById("invoice-description");
const invoiceClientEmailInput = document.getElementById("invoice-client-email");
const invoicePayoutAddressInput = document.getElementById("invoice-payout-address");
const invoiceSummarySubtotal = document.getElementById("invoice-summary-subtotal");
const invoiceSummaryFeeRow = document.getElementById("invoice-summary-fee-row");
const invoiceSummaryFee = document.getElementById("invoice-summary-fee");
const invoiceSummaryTotal = document.getElementById("invoice-summary-total");
const invoiceSubmitButton = invoiceForm.querySelector('button[type="submit"]');

let authMode = "sign-up";
let currentUser = null;
let refreshTimer = null;
let createInvoiceInFlight = false;
let activeInvoiceRequestId = null;

getStartedButton.addEventListener("click", () => {
  showAuth("sign-up");
});

viewDemoButton.addEventListener("click", async () => {
  landingStatus.textContent = "Opening demo invoice...";
  viewDemoButton.disabled = true;

  try {
    const response = await fetch(`${API_BASE}/public/invoices/${DEMO_INVOICE_ID}`);
    const target = response.ok ? `/pay/${DEMO_INVOICE_ID}` : DEMO_INVOICE_FALLBACK_URL;
    window.location.assign(target);
  } catch (_) {
    window.location.assign(DEMO_INVOICE_FALLBACK_URL);
  } finally {
    viewDemoButton.disabled = false;
  }
});

authToggle.addEventListener("click", () => {
  authMode = authMode === "sign-in" ? "sign-up" : "sign-in";
  renderAuthMode();
});

backToLandingButton.addEventListener("click", () => {
  authForm.reset();
  authStatus.textContent = "";
  showScreen("landing");
});

authForm.addEventListener("submit", async (event) => {
  event.preventDefault();
  authStatus.textContent = authMode === "sign-in" ? "Signing in..." : "Creating account...";

  try {
    const response = await apiRequest(authMode === "sign-in" ? "/auth/sign-in" : "/auth/sign-up", {
      method: "POST",
      body: JSON.stringify({
        email: emailInput.value,
        password: passwordInput.value,
      }),
    });

    localStorage.setItem(TOKEN_KEY, response.token);
    currentUser = response.user;
    authForm.reset();
    authStatus.textContent = "";
    await showDashboard();
  } catch (error) {
    authStatus.textContent = error.message;
  }
});

logoutButton.addEventListener("click", async () => {
  invoiceStatus.textContent = "Signing out...";
  logoutButton.disabled = true;

  try {
    await apiRequest("/auth/logout", {
      method: "POST",
      token: getToken(),
    });
    clearSessionState();
  } catch (error) {
    if (handleUnauthorized(error)) {
      return;
    }
    invoiceStatus.textContent = error.message;
  } finally {
    logoutButton.disabled = false;
  }
});

refreshButton.addEventListener("click", async () => {
  invoiceStatus.textContent = "";

  try {
    await loadInvoices();
  } catch (error) {
    if (handleUnauthorized(error)) {
      return;
    }
    invoiceStatus.textContent = error.message;
  }
});

openInvoiceModalButton.addEventListener("click", () => {
  invoiceStatus.textContent = "";
  openInvoiceModal();
});

closeInvoiceModalButton.addEventListener("click", () => {
  closeInvoiceModal();
});

invoiceModal.addEventListener("click", (event) => {
  if (event.target === invoiceModal) {
    closeInvoiceModal(true);
  }
});

document.addEventListener("keydown", (event) => {
  if (event.key === "Escape" && !createInvoiceInFlight) {
    closeInvoiceModal();
  }
});

invoiceAmountInput.addEventListener("input", updateInvoiceSummary);
invoiceAmountInput.addEventListener("input", resetInvoiceRequestId);
invoiceDescriptionInput.addEventListener("input", resetInvoiceRequestId);
invoiceClientEmailInput.addEventListener("input", resetInvoiceRequestId);
invoicePayoutAddressInput.addEventListener("input", resetInvoiceRequestId);

invoiceForm.addEventListener("submit", async (event) => {
  event.preventDefault();
  if (createInvoiceInFlight) {
    return;
  }

  createInvoiceInFlight = true;
  setInvoiceSubmitting(true);
  invoiceStatus.textContent = "Creating invoice...";

  try {
    const clientRequestId = activeInvoiceRequestId || createClientRequestId();
    activeInvoiceRequestId = clientRequestId;
    const invoice = await apiRequest("/me/invoices", {
      method: "POST",
      body: JSON.stringify({
        client_request_id: clientRequestId,
        amount_usdc: invoiceAmountInput.value,
        description: invoiceDescriptionInput.value,
        client_email: invoiceClientEmailInput.value,
        payout_address: invoicePayoutAddressInput.value,
      }),
      token: getToken(),
    });

    closeInvoiceModal();
    invoiceForm.reset();
    activeInvoiceRequestId = null;
    updateInvoiceSummary();
    await loadInvoices();

    const invoicePath = `/pay/${invoice.id}`;
    const invoiceUrl = new URL(invoicePath, window.location.origin).toString();
    const copyLabel = await copyInvoiceLink(invoiceUrl);
    invoiceStatus.innerHTML = `
      ${copyLabel} <a class="inline-link" href="${invoicePath}" target="_blank" rel="noreferrer">Open pay page</a><br />
      Wallet: <code class="inline-code">${escapeHtml(shortAddress(invoice.wallet_pubkey))}</code><br />
      USDC account: <code class="inline-code">${escapeHtml(shortAddress(invoice.usdc_ata))}</code>
    `;
  } catch (error) {
    if (handleUnauthorized(error)) {
      return;
    }
    invoiceStatus.textContent = error.message;
  } finally {
    createInvoiceInFlight = false;
    setInvoiceSubmitting(false);
  }
});

function renderAuthMode() {
  const signingIn = authMode === "sign-in";
  authSubmit.textContent = signingIn ? "Sign in" : "Create account";
  authSwitchLabel.textContent = signingIn ? "Need an account?" : "Already have one?";
  authToggle.textContent = signingIn ? "Create account" : "Sign in";
  passwordInput.autocomplete = signingIn ? "current-password" : "new-password";
  authStatus.textContent = "";
}

function showScreen(screen) {
  landingScreen.classList.toggle("hidden", screen !== "landing");
  authScreen.classList.toggle("hidden", screen !== "auth");
  dashboardScreen.classList.toggle("hidden", screen !== "dashboard");
}

function showAuth(mode) {
  authMode = mode;
  renderAuthMode();
  showScreen("auth");
}

async function showDashboard() {
  if (!currentUser) {
    currentUser = await apiRequest("/auth/me", { token: getToken() });
  }

  dashboardSubtitle.textContent = `${currentUser.email} · Mainnet USDC`;
  showScreen("dashboard");
  await loadInvoices();
  startRefresh();
}

async function loadInvoices() {
  const invoices = await apiRequest("/me/invoices", { token: getToken() });
  const total = invoices.reduce((sum, invoice) => sum + Number(invoice.paid_amount_usdc || 0), 0);
  totalReceived.textContent = formatMoney(total);

  if (!invoices.length) {
    invoiceList.innerHTML = `
      <div class="empty-state">
        <strong class="empty-state-title">No invoices yet</strong>
        <p class="empty-state-copy">Create your first invoice to get paid in seconds.</p>
      </div>
    `;
    return;
  }

  invoiceList.innerHTML = invoices
    .map((invoice) => {
      const paidAmount = Number(invoice.paid_amount_usdc || 0);
      const statusClass = invoice.status === "paid" ? "paid" : "pending";
      const statusLabel = invoice.status === "paid" ? "Paid" : "Pending";
      const description = invoice.description ? escapeHtml(invoice.description) : "";
      const netAmount = Number(invoice.net_amount_usdc || 0);
      const feeAmount = Number(invoice.platform_fee_usdc || 0);
      const paymentLabel =
        paidAmount > 0
          ? feeAmount > 0
            ? `${formatMoney(paidAmount)} paid · ${formatMoney(netAmount)} after fee`
            : `${formatMoney(paidAmount)} paid`
          : "No payment yet";

      return `
        <article class="invoice-row">
          <div class="invoice-row-main">
            <strong class="money invoice-row-amount">${formatMoney(invoice.amount_usdc)}</strong>
            ${
              description
                ? `<span class="invoice-row-description">${description}</span>`
                : ""
            }
            <span class="invoice-row-subtext">${paymentLabel}</span>
            <span class="invoice-row-subtext invoice-row-routing">Wallet ${escapeHtml(shortAddress(invoice.wallet_pubkey))} · USDC ${escapeHtml(shortAddress(invoice.usdc_ata))}</span>
          </div>
          <div class="invoice-row-status">
            <span class="status-badge ${statusClass}">${statusLabel}</span>
          </div>
          <div class="invoice-row-side">
            <span class="invoice-row-date">${formatShortDate(invoice.created_at)}</span>
            <a class="row-link" href="/pay/${invoice.id}" target="_blank" rel="noreferrer">View</a>
          </div>
        </article>
      `;
    })
    .join("");
}

function startRefresh() {
  stopRefresh();
  refreshTimer = window.setInterval(async () => {
    try {
      await loadInvoices();
    } catch (error) {
      handleUnauthorized(error);
    }
  }, 8000);
}

function stopRefresh() {
  if (refreshTimer) {
    window.clearInterval(refreshTimer);
    refreshTimer = null;
  }
}

function openInvoiceModal() {
  activeInvoiceRequestId = createClientRequestId();
  invoiceModal.classList.remove("hidden");
  setInvoiceSubmitting(false);
  updateInvoiceSummary();
  window.setTimeout(() => invoiceAmountInput.focus(), 0);
}

function closeInvoiceModal(force = false) {
  if (createInvoiceInFlight && !force) {
    return;
  }

  activeInvoiceRequestId = null;
  invoiceModal.classList.add("hidden");
}

function updateInvoiceSummary() {
  const customerPaysValue = parseAmount(invoiceAmountInput.value);
  const feeValue = roundToSix(customerPaysValue * PLATFORM_FEE_RATE);
  const totalValue = Math.max(0, roundToSix(customerPaysValue - feeValue));

  invoiceSummarySubtotal.textContent = formatMoney(customerPaysValue);
  invoiceSummaryFee.textContent = formatMoney(feeValue);
  invoiceSummaryTotal.textContent = formatMoney(totalValue);
  invoiceSummaryFeeRow.classList.toggle("hidden", feeValue <= 0);
}

function parseAmount(value) {
  const parsed = Number.parseFloat(value);
  if (!Number.isFinite(parsed) || parsed <= 0) {
    return 0;
  }
  return roundToSix(parsed);
}

async function copyInvoiceLink(invoiceUrl) {
  if (!navigator.clipboard || typeof navigator.clipboard.writeText !== "function") {
    return "Invoice created.";
  }

  try {
    await navigator.clipboard.writeText(invoiceUrl);
    return "Invoice link copied \u2713";
  } catch (_) {
    return "Invoice created.";
  }
}

function roundToSix(value) {
  return Math.round(value * 1_000_000) / 1_000_000;
}

function formatMoney(value) {
  const amount = Number(value || 0);
  return new Intl.NumberFormat("en-US", {
    style: "currency",
    currency: "USD",
    minimumFractionDigits: 2,
    maximumFractionDigits: 5,
  }).format(amount);
}

function formatShortDate(value) {
  return new Date(value).toLocaleDateString("en-US", {
    month: "short",
    day: "numeric",
  });
}

function escapeHtml(value) {
  return String(value)
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll('"', "&quot;")
    .replaceAll("'", "&#39;");
}

function shortAddress(value) {
  if (!value) {
    return "-";
  }

  return `${value.slice(0, 4)}...${value.slice(-4)}`;
}

function resetInvoiceRequestId() {
  if (!createInvoiceInFlight) {
    activeInvoiceRequestId = null;
  }
}

function setInvoiceSubmitting(submitting) {
  invoiceSubmitButton.disabled = submitting;
  closeInvoiceModalButton.disabled = submitting;
  invoiceSubmitButton.textContent = submitting ? "Creating..." : "Create Invoice";
}

function createClientRequestId() {
  if (window.crypto && typeof window.crypto.randomUUID === "function") {
    return window.crypto.randomUUID();
  }

  return `req-${Date.now()}-${Math.random().toString(16).slice(2)}`;
}

function getToken() {
  return localStorage.getItem(TOKEN_KEY) || "";
}

function handleUnauthorized(error) {
  if (error && error.status === 401) {
    clearSessionState();
    return true;
  }

  return false;
}

function clearSessionState() {
  localStorage.removeItem(TOKEN_KEY);
  currentUser = null;
  stopRefresh();
  closeInvoiceModal();
  invoiceStatus.textContent = "";
  authStatus.textContent = "";
  showScreen("landing");
}

async function apiRequest(path, options = {}) {
  const headers = {
    ...(options.body ? { "Content-Type": "application/json" } : {}),
    ...(options.headers || {}),
  };

  if (options.token) {
    headers.Authorization = `Bearer ${options.token}`;
  }

  const response = await fetch(`${API_BASE}${path}`, {
    method: options.method || "GET",
    headers,
    body: options.body,
  });

  const data = await response.json().catch(() => ({}));

  if (!response.ok) {
    const error = new Error(data.error || "Request failed");
    error.status = response.status;
    throw error;
  }

  return data;
}

async function bootstrap() {
  renderAuthMode();
  updateInvoiceSummary();

  const token = getToken();
  if (!token) {
    showScreen("landing");
    return;
  }

  try {
    await showDashboard();
  } catch (error) {
    clearSessionState();
  }
}

bootstrap();
