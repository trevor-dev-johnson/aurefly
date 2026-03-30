export type PublicInvoice = {
  id: string;
  amount_usdc: string;
  subtotal_usdc?: string;
  platform_fee_usdc?: string;
  platform_fee_bps?: number;
  net_amount_usdc?: string;
  paid_amount_usdc?: string;
  status: "pending" | "paid" | "expired" | "cancelled" | string;
  description?: string | null;
  usdc_ata: string;
  wallet_pubkey?: string | null;
  reference_pubkey?: string | null;
  payment_uri?: string | null;
  payment_observed?: boolean;
  latest_payment_tx_url?: string | null;
  payment_observed_tx_url?: string | null;
};

export type MerchantInvoice = PublicInvoice & {
  created_at: string;
  paid_at?: string | null;
  client_email?: string | null;
  client_request_id?: string | null;
  requested_payout_address?: string | null;
};

export type AuthenticatedUser = {
  id: string;
  email: string;
};

export type CreateInvoicePayload = {
  client_request_id: string;
  amount_usdc: string;
  description?: string;
  client_email?: string;
  payout_address: string;
};

export class ApiError extends Error {
  status: number;

  constructor(message: string, status: number) {
    super(message);
    this.name = "ApiError";
    this.status = status;
  }
}

const DEFAULT_API_URL = "http://localhost:8080";
const API_PREFIX = "/api/v1";

type ApiFetchOptions = {
  method?: string;
  body?: string;
  headers?: HeadersInit;
};

export function getApiBase() {
  return (process.env.NEXT_PUBLIC_API_URL || DEFAULT_API_URL).replace(/\/$/, "");
}

async function apiFetch<T>(path: string, options: ApiFetchOptions = {}) {
  const headers = new Headers(options.headers || {});

  if (options.body && !headers.has("Content-Type")) {
    headers.set("Content-Type", "application/json");
  }

  const response = await fetch(`${getApiBase()}${API_PREFIX}${path}`, {
    method: options.method || "GET",
    headers,
    body: options.body,
    cache: "no-store",
  });

  if (response.status === 204) {
    return undefined as T;
  }

  const data = await response.json().catch(() => ({}));

  if (!response.ok) {
    throw new ApiError(
      typeof data?.error === "string" ? data.error : "Request failed.",
      response.status,
    );
  }

  return data as T;
}

async function appFetch<T>(path: string, options: ApiFetchOptions = {}) {
  const headers = new Headers(options.headers || {});

  if (options.body && !headers.has("Content-Type")) {
    headers.set("Content-Type", "application/json");
  }

  const response = await fetch(path, {
    method: options.method || "GET",
    headers,
    body: options.body,
    cache: "no-store",
    credentials: "same-origin",
  });

  if (response.status === 204) {
    return undefined as T;
  }

  const data = await response.json().catch(() => ({}));

  if (!response.ok) {
    throw new ApiError(
      typeof data?.error === "string" ? data.error : "Request failed.",
      response.status,
    );
  }

  return data as T;
}

export async function fetchPublicInvoice(invoiceId: string) {
  return apiFetch<PublicInvoice>(`/public/invoices/${invoiceId}`);
}

export function formatMoney(value: string | number | null | undefined) {
  const amount = Number(value || 0);

  return new Intl.NumberFormat("en-US", {
    style: "currency",
    currency: "USD",
    minimumFractionDigits: 2,
    maximumFractionDigits: 6,
  }).format(amount);
}

export function shortAddress(value: string | null | undefined) {
  if (!value) {
    return "-";
  }

  return `${value.slice(0, 4)}...${value.slice(-5)}`;
}

export async function fetchCurrentUser() {
  return appFetch<AuthenticatedUser>("/api/me");
}

export async function fetchInvoices() {
  return appFetch<MerchantInvoice[]>("/api/invoices");
}

export async function createInvoice(payload: CreateInvoicePayload) {
  return appFetch<MerchantInvoice>("/api/invoices", {
    method: "POST",
    body: JSON.stringify(payload),
  });
}

export async function cancelInvoice(invoiceId: string) {
  return appFetch<MerchantInvoice>(`/api/invoices/${invoiceId}/cancel`, {
    method: "POST",
  });
}

export function createClientRequestId() {
  if (typeof window !== "undefined" && window.crypto?.randomUUID) {
    return window.crypto.randomUUID();
  }

  return `req-${Date.now()}-${Math.random().toString(16).slice(2)}`;
}

export function invoiceHasRequiredReference(invoice: PublicInvoice | null | undefined) {
  if (!invoice?.reference_pubkey || !invoice?.payment_uri) {
    return false;
  }

  const [, query = ""] = String(invoice.payment_uri).split("?");
  if (!query) {
    return false;
  }

  return new URLSearchParams(query)
    .getAll("reference")
    .includes(String(invoice.reference_pubkey));
}
