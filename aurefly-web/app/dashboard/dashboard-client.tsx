"use client";

import Image from "next/image";
import Link from "next/link";
import { useRouter } from "next/navigation";
import { useEffect, useMemo, useState } from "react";
import {
  ApiError,
  cancelInvoice,
  clearStoredToken,
  createClientRequestId,
  createInvoice,
  fetchInvoices,
  fetchMe,
  formatMoney,
  getStoredToken,
  setStoredToken,
  shortAddress,
  signOut,
  type AuthenticatedUser,
  type MerchantInvoice,
} from "@/lib/aurefly-api";

const POLL_INTERVAL_MS = 8_000;

type CreateInvoiceState = {
  amount_usdc: string;
  description: string;
  client_email: string;
  payout_address: string;
};

const initialInvoiceState: CreateInvoiceState = {
  amount_usdc: "",
  description: "",
  client_email: "",
  payout_address: "",
};

type Notice =
  | {
      type: "success";
      text: string;
      invoiceId?: string;
      walletPubkey?: string | null;
      usdcAta?: string;
    }
  | {
      type: "error";
      text: string;
    }
  | null;

function formatShortDate(value: string) {
  return new Date(value).toLocaleDateString("en-US", {
    month: "short",
    day: "numeric",
  });
}

function roundToSix(value: number) {
  return Math.round(value * 1_000_000) / 1_000_000;
}

function parseAmount(value: string) {
  const parsed = Number.parseFloat(value);
  if (!Number.isFinite(parsed) || parsed <= 0) {
    return 0;
  }

  return roundToSix(parsed);
}

async function copyInvoiceUrl(invoicePath: string) {
  if (!navigator.clipboard?.writeText) {
    return false;
  }

  try {
    await navigator.clipboard.writeText(new URL(invoicePath, window.location.origin).toString());
    return true;
  } catch {
    return false;
  }
}

export function DashboardClient() {
  const router = useRouter();
  const [token, setToken] = useState("");
  const [user, setUser] = useState<AuthenticatedUser | null>(null);
  const [invoices, setInvoices] = useState<MerchantInvoice[]>([]);
  const [loading, setLoading] = useState(true);
  const [refreshing, setRefreshing] = useState(false);
  const [modalOpen, setModalOpen] = useState(false);
  const [createState, setCreateState] = useState<CreateInvoiceState>(initialInvoiceState);
  const [createRequestId, setCreateRequestId] = useState("");
  const [submitting, setSubmitting] = useState(false);
  const [cancellingInvoiceId, setCancellingInvoiceId] = useState("");
  const [notice, setNotice] = useState<Notice>(null);

  useEffect(() => {
    const storedToken = getStoredToken();
    if (!storedToken) {
      router.replace("/auth?mode=sign-in");
      return;
    }

    setStoredToken(storedToken);
    setToken(storedToken);
  }, [router]);

  useEffect(() => {
    if (!token) {
      return;
    }

    let cancelled = false;

    async function bootstrap() {
      setLoading(true);

      try {
        const [nextUser, nextInvoices] = await Promise.all([
          fetchMe(token),
          fetchInvoices(token),
        ]);

        if (cancelled) {
          return;
        }

        setUser(nextUser);
        setInvoices(nextInvoices);
        setNotice(null);
      } catch (error) {
        if (cancelled) {
          return;
        }

        if (error instanceof ApiError && error.status === 401) {
          clearStoredToken();
          router.replace("/auth?mode=sign-in");
          return;
        }

        setNotice({
          type: "error",
          text: error instanceof Error ? error.message : "Unable to load dashboard.",
        });
      } finally {
        if (!cancelled) {
          setLoading(false);
        }
      }
    }

    void bootstrap();

    return () => {
      cancelled = true;
    };
  }, [router, token]);

  useEffect(() => {
    if (!token || !user) {
      return;
    }

    const interval = window.setInterval(async () => {
      try {
        const nextInvoices = await fetchInvoices(token);
        setInvoices(nextInvoices);
      } catch (error) {
        if (error instanceof ApiError && error.status === 401) {
          clearStoredToken();
          router.replace("/auth?mode=sign-in");
        }
      }
    }, POLL_INTERVAL_MS);

    return () => window.clearInterval(interval);
  }, [router, token, user]);

  useEffect(() => {
    if (!modalOpen) {
      return;
    }

    const previousBodyOverflow = document.body.style.overflow;
    const previousHtmlOverflow = document.documentElement.style.overflow;
    const previousBodyOverscroll = document.body.style.overscrollBehavior;
    const previousHtmlOverscroll = document.documentElement.style.overscrollBehavior;

    document.body.style.overflow = "hidden";
    document.documentElement.style.overflow = "hidden";
    document.body.style.overscrollBehavior = "none";
    document.documentElement.style.overscrollBehavior = "none";

    return () => {
      document.body.style.overflow = previousBodyOverflow;
      document.documentElement.style.overflow = previousHtmlOverflow;
      document.body.style.overscrollBehavior = previousBodyOverscroll;
      document.documentElement.style.overscrollBehavior = previousHtmlOverscroll;
    };
  }, [modalOpen]);

  const metrics = useMemo(() => {
    const totalReceived = invoices.reduce(
      (sum, invoice) => sum + Number(invoice.paid_amount_usdc || 0),
      0,
    );
    const pendingTotal = invoices.reduce((sum, invoice) => {
      if (invoice.status !== "pending") {
        return sum;
      }

      return (
        sum +
        Math.max(0, Number(invoice.amount_usdc || 0) - Number(invoice.paid_amount_usdc || 0))
      );
    }, 0);
    const paidCount = invoices.filter((invoice) => invoice.status === "paid").length;
    const pendingCount = invoices.filter((invoice) => invoice.status === "pending").length;

    return {
      totalReceived,
      pendingTotal,
      paidCount,
      pendingCount,
    };
  }, [invoices]);

  const summaryAmount = useMemo(() => parseAmount(createState.amount_usdc), [createState.amount_usdc]);

  async function refreshInvoices() {
    if (!token) {
      return;
    }

    setRefreshing(true);

    try {
      const nextInvoices = await fetchInvoices(token);
      setInvoices(nextInvoices);
      setNotice(null);
    } catch (error) {
      if (error instanceof ApiError && error.status === 401) {
        clearStoredToken();
        router.replace("/auth?mode=sign-in");
        return;
      }

      setNotice({
        type: "error",
        text: error instanceof Error ? error.message : "Unable to refresh invoices.",
      });
    } finally {
      setRefreshing(false);
    }
  }

  function openModal() {
    setCreateRequestId(createClientRequestId());
    setCreateState(initialInvoiceState);
    setModalOpen(true);
  }

  function closeModal() {
    if (submitting) {
      return;
    }

    setModalOpen(false);
    setCreateState(initialInvoiceState);
  }

  function updateField<K extends keyof CreateInvoiceState>(key: K, value: CreateInvoiceState[K]) {
    setCreateState((current) => ({ ...current, [key]: value }));
    if (!submitting) {
      setCreateRequestId("");
    }
  }

  async function handleCreateInvoice(event: React.FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (!token || submitting) {
      return;
    }

    setSubmitting(true);
    setNotice(null);

    try {
      const clientRequestId = createRequestId || createClientRequestId();
      const invoice = await createInvoice(
        {
          client_request_id: clientRequestId,
          amount_usdc: createState.amount_usdc,
          description: createState.description.trim(),
          client_email: createState.client_email.trim(),
          payout_address: createState.payout_address.trim(),
        },
        token,
      );

      const nextInvoices = await fetchInvoices(token);
      setInvoices(nextInvoices);
      setModalOpen(false);
      setCreateState(initialInvoiceState);
      setCreateRequestId("");

      const invoicePath = `/pay/${invoice.id}`;
      const copied = await copyInvoiceUrl(invoicePath);
      setNotice({
        type: "success",
        text: copied ? "Invoice link copied." : "Invoice created.",
        invoiceId: invoice.id,
        walletPubkey: invoice.wallet_pubkey,
        usdcAta: invoice.usdc_ata,
      });
    } catch (error) {
      if (error instanceof ApiError && error.status === 401) {
        clearStoredToken();
        router.replace("/auth?mode=sign-in");
        return;
      }

      setNotice({
        type: "error",
        text: error instanceof Error ? error.message : "Unable to create invoice.",
      });
    } finally {
      setSubmitting(false);
    }
  }

  async function handleSignOut() {
    if (!token) {
      clearStoredToken();
      router.replace("/");
      return;
    }

    try {
      await signOut(token);
    } catch (error) {
      if (!(error instanceof ApiError) || error.status !== 401) {
        setNotice({
          type: "error",
          text: error instanceof Error ? error.message : "Unable to sign out.",
        });
        return;
      }
    }

    clearStoredToken();
    router.replace("/");
  }

  async function handleCancelInvoice(invoiceId: string) {
    if (!token || cancellingInvoiceId || !window.confirm("Cancel this invoice?")) {
      return;
    }

    setCancellingInvoiceId(invoiceId);
    setNotice(null);

    try {
      const cancelledInvoice = await cancelInvoice(invoiceId, token);
      const nextInvoices = await fetchInvoices(token);
      setInvoices(nextInvoices);
      setNotice({
        type: "success",
        text: `Invoice ${cancelledInvoice.id.slice(0, 8).toUpperCase()} cancelled.`,
      });
    } catch (error) {
      if (error instanceof ApiError && error.status === 401) {
        clearStoredToken();
        router.replace("/auth?mode=sign-in");
        return;
      }

      setNotice({
        type: "error",
        text: error instanceof Error ? error.message : "Unable to cancel invoice.",
      });
    } finally {
      setCancellingInvoiceId("");
    }
  }

  if (loading) {
    return (
      <main className="relative flex min-h-screen items-center justify-center overflow-hidden px-6 py-10">
        <div className="pointer-events-none absolute inset-x-0 top-[-10rem] mx-auto h-[28rem] w-[min(92vw,56rem)] rounded-full bg-[radial-gradient(circle,rgba(90,141,255,0.16),rgba(77,223,143,0.08)_44%,transparent_74%)] blur-3xl" />
        <div className="rounded-[1.75rem] border border-white/8 bg-[linear-gradient(180deg,rgba(255,255,255,0.05),rgba(255,255,255,0.02))] px-8 py-6 text-sm text-slate-300 shadow-[0_28px_90px_rgba(0,0,0,0.28)] backdrop-blur-xl">
          Loading dashboard...
        </div>
      </main>
    );
  }

  return (
    <main className="relative min-h-screen overflow-hidden px-4 py-4 sm:px-6 sm:py-6">
      <div className="pointer-events-none absolute inset-x-0 top-[-10rem] mx-auto h-[28rem] w-[min(92vw,64rem)] rounded-full bg-[radial-gradient(circle,rgba(90,141,255,0.16),rgba(77,223,143,0.08)_44%,transparent_74%)] blur-3xl" />
      <div className="pointer-events-none absolute bottom-[-10rem] left-1/2 h-[24rem] w-[24rem] -translate-x-1/2 rounded-full bg-[radial-gradient(circle,rgba(248,211,111,0.12),transparent_72%)] blur-3xl" />

      <div className="relative mx-auto grid min-h-[calc(100vh-2rem)] max-w-7xl gap-4 lg:grid-cols-[260px_minmax(0,1fr)]">
        <aside className="rounded-[2rem] border border-white/8 bg-[linear-gradient(180deg,rgba(255,255,255,0.045),rgba(255,255,255,0.02))] p-5 shadow-[0_30px_100px_rgba(0,0,0,0.28)] backdrop-blur-2xl">
          <div className="flex items-center gap-3">
            <Image
              src="/aurefly-logo.svg"
              alt="Aurefly"
              width={38}
              height={38}
              className="h-9 w-9 drop-shadow-[0_0_18px_rgba(248,211,111,0.2)]"
              priority
            />
            <div>
              <div className="font-semibold tracking-[-0.03em] text-white">Aurefly</div>
              <div className="text-xs uppercase tracking-[0.24em] text-slate-500">
                Dashboard
              </div>
            </div>
          </div>

          <nav className="mt-10 grid gap-2 text-sm text-slate-300">
            <a href="#overview" className="rounded-2xl bg-white/[0.04] px-4 py-3 text-white">
              Overview
            </a>
            <a href="#invoices" className="rounded-2xl px-4 py-3 transition hover:bg-white/[0.04]">
              Invoices
            </a>
            <a href="#wallet" className="rounded-2xl px-4 py-3 transition hover:bg-white/[0.04]">
              Wallet
            </a>
          </nav>

          <div className="mt-10 rounded-[1.5rem] border border-white/7 bg-white/[0.03] p-4">
            <div className="text-xs uppercase tracking-[0.22em] text-slate-500">Account</div>
            <div className="mt-3 text-sm font-medium text-white">{user?.email}</div>
            <div className="mt-1 text-sm text-slate-400">Solana mainnet</div>
          </div>

          <button
            type="button"
            onClick={handleSignOut}
            className="mt-4 inline-flex h-11 w-full items-center justify-center rounded-full border border-white/10 bg-white/[0.03] px-5 text-sm font-medium text-slate-100 transition hover:border-white/20 hover:bg-white/[0.05]"
          >
            Sign out
          </button>
        </aside>

        <section className="rounded-[2rem] border border-white/8 bg-[linear-gradient(180deg,rgba(255,255,255,0.045),rgba(255,255,255,0.02))] p-5 shadow-[0_30px_100px_rgba(0,0,0,0.28)] backdrop-blur-2xl sm:p-6">
          <header
            id="overview"
            className="flex flex-col gap-4 border-b border-white/6 pb-6 lg:flex-row lg:items-center lg:justify-between"
          >
            <div>
              <p className="font-mono text-[11px] uppercase tracking-[0.28em] text-slate-500">
                Overview
              </p>
              <h1 className="mt-3 text-3xl font-semibold tracking-[-0.05em] text-white sm:text-4xl">
                Manage invoices without leaving your wallet flow.
              </h1>
              <p className="mt-4 max-w-2xl text-sm leading-7 text-slate-300">
                Create USDC invoices, share a clean payment page, and let Aurefly confirm
                settlement directly to your Solana wallet.
              </p>
            </div>

            <div className="flex flex-col gap-3 sm:flex-row">
              <button
                type="button"
                onClick={refreshInvoices}
                className="inline-flex h-11 items-center justify-center rounded-full border border-white/10 bg-white/[0.03] px-5 text-sm font-medium text-slate-100 transition hover:border-white/20 hover:bg-white/[0.05]"
              >
                {refreshing ? "Refreshing..." : "Refresh"}
              </button>
              <button
                type="button"
                onClick={openModal}
                className="inline-flex h-11 items-center justify-center rounded-full bg-[#4f86ff] px-5 text-sm font-semibold text-white shadow-[0_12px_30px_rgba(79,134,255,0.24)] transition hover:-translate-y-px hover:bg-[#6595ff]"
              >
                New Invoice
              </button>
            </div>
          </header>

          <section className="mt-6 grid gap-4 xl:grid-cols-3">
            <article className="rounded-[1.7rem] border border-emerald-400/14 bg-emerald-400/8 p-5">
              <div className="text-sm text-emerald-100/80">Total earned</div>
              <div className="mt-4 text-3xl font-semibold tracking-[-0.05em] text-white">
                {formatMoney(metrics.totalReceived)}
              </div>
              <div className="mt-2 text-sm text-emerald-100/70">USDC settled</div>
            </article>
            <article className="rounded-[1.7rem] border border-sky-400/14 bg-sky-400/8 p-5">
              <div className="text-sm text-sky-100/80">Pending</div>
              <div className="mt-4 text-3xl font-semibold tracking-[-0.05em] text-white">
                {formatMoney(metrics.pendingTotal)}
              </div>
              <div className="mt-2 text-sm text-sky-100/70">
                {metrics.pendingCount} open invoices
              </div>
            </article>
            <article className="rounded-[1.7rem] border border-white/8 bg-white/[0.03] p-5">
              <div className="text-sm text-slate-300">Paid</div>
              <div className="mt-4 text-3xl font-semibold tracking-[-0.05em] text-white">
                {metrics.paidCount}
              </div>
              <div className="mt-2 text-sm text-slate-400">Invoices confirmed</div>
            </article>
          </section>

          {notice ? (
            <section
              className={`mt-6 rounded-[1.6rem] border p-5 ${
                notice.type === "success"
                  ? "border-emerald-400/18 bg-emerald-400/8"
                  : "border-rose-400/16 bg-rose-400/8"
              }`}
            >
              <p className="text-sm leading-7 text-white">{notice.text}</p>
              {notice.type === "success" && notice.invoiceId ? (
                <div className="mt-3 text-sm leading-7 text-slate-200">
                  <Link
                    href={`/pay/${notice.invoiceId}`}
                    target="_blank"
                    className="font-medium text-sky-200 transition hover:text-white"
                  >
                    Open pay page
                  </Link>
                  <div className="mt-2">
                    Wallet:{" "}
                    <code className="rounded-lg bg-black/20 px-2 py-1 font-mono text-xs text-white">
                      {shortAddress(notice.walletPubkey)}
                    </code>
                  </div>
                  <div className="mt-2">
                    USDC account:{" "}
                    <code className="rounded-lg bg-black/20 px-2 py-1 font-mono text-xs text-white">
                      {shortAddress(notice.usdcAta)}
                    </code>
                  </div>
                </div>
              ) : null}
            </section>
          ) : null}

          <section
            id="invoices"
            className="mt-6 rounded-[1.8rem] border border-white/7 bg-white/[0.03] p-4 sm:p-5"
          >
            <div className="flex flex-col gap-3 border-b border-white/6 pb-5 sm:flex-row sm:items-end sm:justify-between">
              <div>
                <h2 className="text-xl font-semibold tracking-[-0.03em] text-white">Invoices</h2>
                <p className="mt-2 text-sm leading-7 text-slate-400">
                  Share the Aurefly payment link or QR. Manual transfers may not be credited automatically.
                </p>
              </div>
              <div className="font-mono text-[11px] uppercase tracking-[0.26em] text-slate-500">
                {invoices.length} total
              </div>
            </div>

            {invoices.length === 0 ? (
              <div className="py-14 text-center">
                <div className="text-lg font-semibold tracking-[-0.03em] text-white">
                  No invoices yet
                </div>
                <p className="mt-3 text-sm leading-7 text-slate-400">
                  Create your first invoice to get paid in seconds.
                </p>
              </div>
            ) : (
              <div className="mt-5 grid gap-3">
                {invoices.map((invoice) => {
                  const paidAmount = Number(invoice.paid_amount_usdc || 0);
                  const feeAmount = Number(invoice.platform_fee_usdc || 0);
                  const netAmount = Number(invoice.net_amount_usdc || 0);
                  const paymentLabel =
                    invoice.status === "cancelled"
                      ? "Cancelled"
                      : paidAmount > 0
                      ? feeAmount > 0
                        ? `${formatMoney(paidAmount)} paid · ${formatMoney(netAmount)} after fee`
                        : `${formatMoney(paidAmount)} paid`
                      : "No payment yet";

                  return (
                    <article
                      key={invoice.id}
                      className="grid gap-4 rounded-[1.4rem] border border-white/7 bg-[#0c1520]/80 p-4 lg:grid-cols-[110px_minmax(0,1fr)_160px_180px]"
                    >
                      <div>
                        <div className="font-mono text-[11px] uppercase tracking-[0.24em] text-slate-500">
                          Invoice
                        </div>
                        <div className="mt-2 text-sm font-semibold text-white">
                          {invoice.id.slice(0, 8).toUpperCase()}
                        </div>
                        <div className="mt-2 text-xs text-slate-500">
                          {formatShortDate(invoice.created_at)}
                        </div>
                      </div>

                      <div>
                        <div className="text-sm font-medium text-white">
                          {invoice.client_email || "Direct invoice"}
                        </div>
                        {invoice.description ? (
                          <div className="mt-2 text-sm leading-7 text-slate-300">
                            {invoice.description}
                          </div>
                        ) : null}
                        <div className="mt-2 text-sm text-slate-500">{paymentLabel}</div>
                      </div>

                      <div className="lg:text-right">
                        <div className="text-xs uppercase tracking-[0.2em] text-slate-500">
                          Amount
                        </div>
                        <div className="mt-2 text-lg font-semibold text-white">
                          {formatMoney(invoice.amount_usdc)}
                        </div>
                        <div className="mt-1 text-xs text-slate-500">USDC</div>
                      </div>

                      <div className="flex items-center justify-between gap-4 lg:flex-col lg:items-end lg:justify-center">
                        <span
                          className={`inline-flex rounded-full px-3 py-1 text-xs font-semibold ${
                            invoice.status === "paid"
                              ? "border border-emerald-400/18 bg-emerald-400/10 text-emerald-200"
                              : invoice.status === "cancelled"
                                ? "border border-rose-400/18 bg-rose-400/10 text-rose-200"
                              : "border border-white/10 bg-white/[0.05] text-slate-300"
                          }`}
                        >
                          {invoice.status === "paid"
                            ? "Paid"
                            : invoice.status === "cancelled"
                              ? "Cancelled"
                              : "Pending"}
                        </span>
                        <div className="flex items-center gap-4 lg:flex-col lg:items-end">
                          {invoice.status === "pending" ? (
                            <button
                              type="button"
                              onClick={() => void handleCancelInvoice(invoice.id)}
                              disabled={cancellingInvoiceId === invoice.id}
                              className="text-sm font-medium text-rose-200 transition hover:text-rose-100 disabled:cursor-not-allowed disabled:opacity-60"
                            >
                              {cancellingInvoiceId === invoice.id ? "Cancelling..." : "Cancel"}
                            </button>
                          ) : null}
                          <Link
                            href={`/pay/${invoice.id}`}
                            target="_blank"
                            className="text-sm font-medium text-sky-300 transition hover:text-sky-200"
                          >
                            View
                          </Link>
                        </div>
                      </div>
                    </article>
                  );
                })}
              </div>
            )}
          </section>

          <section
            id="wallet"
            className="mt-6 flex flex-col gap-3 rounded-[1.8rem] border border-white/7 bg-white/[0.03] p-5 sm:flex-row sm:items-center sm:justify-between"
          >
            <div>
              <div className="text-xs uppercase tracking-[0.22em] text-slate-500">
                Connected account
              </div>
              <div className="mt-3 text-lg font-semibold tracking-[-0.03em] text-white">
                {user?.email}
              </div>
            </div>
            <div className="rounded-full border border-white/8 bg-white/[0.03] px-4 py-2 text-sm text-slate-300">
              Solana mainnet
            </div>
          </section>
        </section>
      </div>

      {modalOpen ? (
        <div className="fixed inset-0 z-50 overflow-y-auto bg-[#03070d]/88 px-4 py-4 backdrop-blur-md sm:px-6 sm:py-6">
          <div className="flex min-h-full items-start justify-center sm:items-center">
            <section className="flex w-full max-w-xl flex-col overflow-hidden rounded-[2rem] border border-white/8 bg-[linear-gradient(180deg,rgba(255,255,255,0.055),rgba(255,255,255,0.02))] shadow-[0_34px_100px_rgba(0,0,0,0.38)] backdrop-blur-2xl max-h-[calc(100svh-2rem)] sm:max-h-[min(52rem,calc(100svh-3rem))]">
            <div className="flex items-start justify-between gap-4 border-b border-white/6 px-5 pb-5 pt-5 sm:px-7 sm:pb-6 sm:pt-6">
              <div>
                <div className="font-mono text-[11px] uppercase tracking-[0.26em] text-slate-500">
                  Create invoice
                </div>
                <h2 className="mt-3 text-2xl font-semibold tracking-[-0.04em] text-white">
                  New invoice
                </h2>
                <p className="mt-3 text-sm leading-7 text-slate-300">
                  Enter the amount, add context, and set the destination wallet for settlement.
                </p>
              </div>
              <button
                type="button"
                onClick={closeModal}
                className="inline-flex h-10 w-10 items-center justify-center rounded-full border border-white/10 bg-white/[0.03] text-lg text-white transition hover:bg-white/[0.05]"
              >
                ×
              </button>
            </div>

            <form onSubmit={handleCreateInvoice} className="flex min-h-0 flex-1 flex-col">
              <div className="grid min-h-0 flex-1 gap-5 overflow-y-auto px-5 py-5 sm:px-7 sm:py-6">
              <label className="grid gap-2 text-sm text-slate-300">
                <span>Amount (USDC)</span>
                <div className="flex h-14 items-center rounded-[1.35rem] border border-white/8 bg-[#0d1520]/92 px-4">
                  <span className="pr-3 text-slate-500">$</span>
                  <input
                    type="number"
                    min="0.01"
                    step="0.000001"
                    required
                    value={createState.amount_usdc}
                    onChange={(event) => updateField("amount_usdc", event.target.value)}
                    className="h-full flex-1 bg-transparent text-lg font-semibold text-white outline-none placeholder:text-slate-500"
                    placeholder="0.00"
                  />
                  <span className="pl-3 text-xs uppercase tracking-[0.18em] text-slate-500">
                    USDC
                  </span>
                </div>
              </label>

              <label className="grid gap-2 text-sm text-slate-300">
                <span>Description</span>
                <input
                  type="text"
                  value={createState.description}
                  onChange={(event) => updateField("description", event.target.value)}
                  className="h-12 rounded-2xl border border-white/8 bg-[#0d1520]/92 px-4 text-white outline-none transition placeholder:text-slate-500 focus:border-sky-300/40 focus:bg-[#111b28]"
                  placeholder="e.g. Brand design project — Phase 1"
                />
              </label>

              <label className="grid gap-2 text-sm text-slate-300">
                <span>Client email (optional)</span>
                <input
                  type="email"
                  value={createState.client_email}
                  onChange={(event) => updateField("client_email", event.target.value)}
                  className="h-12 rounded-2xl border border-white/8 bg-[#0d1520]/92 px-4 text-white outline-none transition placeholder:text-slate-500 focus:border-sky-300/40 focus:bg-[#111b28]"
                  placeholder="client@example.com"
                />
              </label>

              <label className="grid gap-2 text-sm text-slate-300">
                <span>Recipient wallet</span>
                <input
                  type="text"
                  required
                  value={createState.payout_address}
                  onChange={(event) => updateField("payout_address", event.target.value)}
                  className="h-12 rounded-2xl border border-white/8 bg-[#0d1520]/92 px-4 text-white outline-none transition placeholder:text-slate-500 focus:border-sky-300/40 focus:bg-[#111b28]"
                  placeholder="Solana wallet address"
                />
                <span className="text-sm leading-6 text-slate-500">
                  Paste a wallet address or USDC account. Aurefly will derive and use the mainnet USDC account automatically.
                </span>
              </label>

              <div className="rounded-[1.5rem] border border-white/7 bg-white/[0.03] p-5">
                <div className="flex items-center justify-between gap-4 text-sm text-slate-300">
                  <span>Customer pays</span>
                  <span className="font-medium text-white">{formatMoney(summaryAmount)}</span>
                </div>
                <div className="mt-3 flex items-center justify-between gap-4 text-sm text-slate-300">
                  <span>Aurefly fee</span>
                  <span className="font-medium text-white">$0.00</span>
                </div>
                <div className="mt-4 flex items-center justify-between gap-4 border-t border-white/6 pt-4 text-sm text-slate-300">
                  <span>You receive</span>
                  <span className="text-lg font-semibold text-white">
                    {formatMoney(summaryAmount)}
                  </span>
                </div>
              </div>
              </div>

              <div className="grid gap-3 border-t border-white/6 px-5 py-4 sm:grid-cols-2 sm:px-7 sm:py-5">
                <button
                  type="button"
                  onClick={closeModal}
                  className="inline-flex h-12 items-center justify-center rounded-full border border-white/10 bg-white/[0.03] px-6 text-sm font-medium text-slate-100 transition hover:border-white/20 hover:bg-white/[0.05]"
                >
                  Cancel
                </button>
                <button
                  type="submit"
                  disabled={submitting}
                  className="inline-flex h-12 items-center justify-center rounded-full bg-[#4f86ff] px-6 text-sm font-semibold text-white shadow-[0_12px_30px_rgba(79,134,255,0.24)] transition hover:-translate-y-px hover:bg-[#6595ff] disabled:cursor-not-allowed disabled:opacity-70"
                >
                  {submitting ? "Creating..." : "Create Invoice"}
                </button>
              </div>
            </form>
          </section>
          </div>
        </div>
      ) : null}
    </main>
  );
}
