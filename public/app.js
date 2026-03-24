const API_BASE = "/api/v1";
const TOKEN_KEY = "aurefly_auth_token";
const PLATFORM_FEE_RATE = 0;

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

let authMode = "sign-up";
let currentUser = null;
let refreshTimer = null;

getStartedButton.addEventListener("click", () => {
  showAuth("sign-up");
});

viewDemoButton.addEventListener("click", async () => {
  landingStatus.textContent = "";
  showAuth("sign-in");
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

logoutButton.addEventListener("click", () => {
  localStorage.removeItem(TOKEN_KEY);
  currentUser = null;
  stopRefresh();
  closeInvoiceModal();
  showScreen("landing");
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
    closeInvoiceModal();
  }
});

document.addEventListener("keydown", (event) => {
  if (event.key === "Escape") {
    closeInvoiceModal();
  }
});

invoiceAmountInput.addEventListener("input", updateInvoiceSummary);

invoiceForm.addEventListener("submit", async (event) => {
  event.preventDefault();
  invoiceStatus.textContent = "Creating invoice...";

  try {
    const invoice = await apiRequest("/me/invoices", {
      method: "POST",
      body: JSON.stringify({
        amount_usdc: invoiceAmountInput.value,
        description: invoiceDescriptionInput.value,
        client_email: invoiceClientEmailInput.value,
        payout_address: invoicePayoutAddressInput.value,
      }),
      token: getToken(),
    });

    closeInvoiceModal();
    invoiceForm.reset();
    updateInvoiceSummary();
    await loadInvoices();

    const invoicePath = `/pay/${invoice.id}`;
    const invoiceUrl = new URL(invoicePath, window.location.origin).toString();
    const copyLabel = await copyInvoiceLink(invoiceUrl);
    invoiceStatus.innerHTML = `${copyLabel} <a class="inline-link" href="${invoicePath}" target="_blank" rel="noreferrer">Open pay page</a>`;
  } catch (error) {
    if (handleUnauthorized(error)) {
      return;
    }
    invoiceStatus.textContent = error.message;
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
  invoiceModal.classList.remove("hidden");
  updateInvoiceSummary();
  window.setTimeout(() => invoiceAmountInput.focus(), 0);
}

function closeInvoiceModal() {
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

function getToken() {
  return localStorage.getItem(TOKEN_KEY) || "";
}

function handleUnauthorized(error) {
  if (error && error.status === 401) {
    localStorage.removeItem(TOKEN_KEY);
    currentUser = null;
    stopRefresh();
    closeInvoiceModal();
    showScreen("landing");
    return true;
  }

  return false;
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
    localStorage.removeItem(TOKEN_KEY);
    currentUser = null;
    showScreen("landing");
  }
}

bootstrap();
