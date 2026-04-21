export type PublicInvoice = {
  id: string;
  amount_usdc: string;
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
  subtotal_usdc?: string;
  platform_fee_usdc?: string;
  platform_fee_bps?: number;
  net_amount_usdc?: string;
};

export type AuthenticatedUser = {
  id: string;
  email: string;
  name?: string | null;
  is_admin?: boolean;
};

export type UnmatchedPaymentSummary = {
  id: string;
  signature: string;
  destination_wallet: string;
  amount_usdc: string;
  sender_wallet?: string | null;
  reference_pubkey?: string | null;
  seen_at: string;
  reason: string;
  status:
    | "pending"
    | "reviewed"
    | "resolved"
    | "ignored"
    | "refunded_manually"
    | "needs_investigation"
    | string;
  linked_invoice_id?: string | null;
  notes?: string | null;
};

export type UnmatchedPaymentAuditEvent = {
  id: string;
  action: string;
  actor_email: string;
  previous_status?: string | null;
  next_status?: string | null;
  linked_invoice_id?: string | null;
  note?: string | null;
  metadata: Record<string, unknown>;
  created_at: string;
};

export type ReconcilePaymentSummary = {
  invoice_id: string;
  tx_signature: string;
  amount_usdc: string;
  payer_wallet_address?: string | null;
  recipient_token_account: string;
  token_mint: string;
  finalized_at?: string | null;
  created_at: string;
};

export type ChainSnapshot = {
  amount_usdc: string;
  source_owner?: string | null;
  finalized_at?: string | null;
  account_keys: string[];
  lookup_error?: string | null;
};

export type UnmatchedPaymentDetail = {
  payment: UnmatchedPaymentSummary;
  linked_invoice?: MerchantInvoice | null;
  existing_payment?: ReconcilePaymentSummary | null;
  audit_events: UnmatchedPaymentAuditEvent[];
  metadata: Record<string, unknown>;
  chain_snapshot?: ChainSnapshot | null;
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

async function internalApiFetch<T>(path: string, options: ApiFetchOptions = {}) {
  const headers = new Headers(options.headers || {});

  if (options.body && !headers.has("Content-Type")) {
    headers.set("Content-Type", "application/json");
  }

  const response = await fetch(`/api${path}`, {
    method: options.method || "GET",
    headers,
    body: options.body,
    cache: "no-store",
    credentials: "include",
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
  return internalApiFetch<AuthenticatedUser>("/me");
}

export async function fetchInvoices() {
  return internalApiFetch<MerchantInvoice[]>("/invoices");
}

export async function createInvoice(payload: CreateInvoicePayload) {
  return internalApiFetch<MerchantInvoice>("/invoices", {
    method: "POST",
    body: JSON.stringify(payload),
  });
}

export async function cancelInvoice(invoiceId: string) {
  return internalApiFetch<MerchantInvoice>(`/invoices/${invoiceId}/cancel`, {
    method: "POST",
  });
}

export async function fetchUnmatchedPayments(query?: URLSearchParams | string) {
  const suffix = query
    ? `?${typeof query === "string" ? query : query.toString()}`
    : "";
  return internalApiFetch<UnmatchedPaymentSummary[]>(`/admin/unmatched-payments${suffix}`);
}

export async function fetchUnmatchedPaymentDetail(unmatchedPaymentId: string) {
  return internalApiFetch<UnmatchedPaymentDetail>(`/admin/unmatched-payments/${unmatchedPaymentId}`);
}

export async function linkUnmatchedPayment(
  unmatchedPaymentId: string,
  payload: { invoice_id: string; note?: string },
) {
  return internalApiFetch<UnmatchedPaymentDetail>(
    `/admin/unmatched-payments/${unmatchedPaymentId}/link`,
    {
      method: "POST",
      body: JSON.stringify(payload),
    },
  );
}

export async function updateUnmatchedPaymentStatus(
  unmatchedPaymentId: string,
  payload: { status: string; note?: string },
) {
  return internalApiFetch<UnmatchedPaymentDetail>(
    `/admin/unmatched-payments/${unmatchedPaymentId}/status`,
    {
      method: "POST",
      body: JSON.stringify(payload),
    },
  );
}

export async function retryUnmatchedPayment(unmatchedPaymentId: string) {
  return internalApiFetch<UnmatchedPaymentDetail>(
    `/admin/unmatched-payments/${unmatchedPaymentId}/retry`,
    {
      method: "POST",
    },
  );
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
