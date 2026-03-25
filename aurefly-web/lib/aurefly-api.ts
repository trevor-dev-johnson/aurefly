export type PublicInvoice = {
  id: string;
  amount_usdc: string;
  subtotal_usdc?: string;
  platform_fee_usdc?: string;
  platform_fee_bps?: number;
  net_amount_usdc?: string;
  paid_amount_usdc?: string;
  status: "pending" | "paid" | "expired" | string;
  description?: string | null;
  usdc_ata: string;
  wallet_pubkey?: string | null;
  reference_pubkey?: string | null;
  payment_uri?: string | null;
  payment_observed?: boolean;
  latest_payment_tx_url?: string | null;
  payment_observed_tx_url?: string | null;
};

const DEFAULT_API_URL = "http://localhost:8080";

export function getApiBase() {
  return (process.env.NEXT_PUBLIC_API_URL || DEFAULT_API_URL).replace(/\/$/, "");
}

export async function fetchPublicInvoice(invoiceId: string, observePayment = true) {
  const url = new URL(`/api/v1/public/invoices/${invoiceId}`, getApiBase());
  if (observePayment) {
    url.searchParams.set("observe_payment", "true");
  }

  const response = await fetch(url.toString(), {
    cache: "no-store",
  });

  const data = await response.json().catch(() => ({}));

  if (!response.ok) {
    throw new Error(
      typeof data?.error === "string" ? data.error : "Unable to load invoice.",
    );
  }

  return data as PublicInvoice;
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
