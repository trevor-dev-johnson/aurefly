"use client";

import Image from "next/image";
import Link from "next/link";
import { useRouter } from "next/navigation";
import { useEffect, useMemo, useState } from "react";
import {
  ApiError,
  cancelInvoice,
  createClientRequestId,
  createInvoice,
  fetchInvoices,
  fetchCurrentUser,
  formatMoney,
  type AuthenticatedUser,
  type MerchantInvoice,
} from "@/lib/aurefly-api";
import { createClient as createSupabaseBrowserClient } from "@/lib/supabase/browser";

const POLL_INTERVAL_MS = 8_000;
const SECTION_IDS = ["overview", "invoices", "wallet"] as const;

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
    }
  | {
      type: "error";
      text: string;
    }
  | null;

type ActivityItem = {
  id: string;
  title: string;
  detail: string;
  time: string;
  tone: "success" | "pending" | "neutral";
};

function formatShortDate(value: string) {
  return new Date(value).toLocaleDateString("en-US", {
    month: "short",
    day: "numeric",
  });
}

function formatRelativeTime(value: string | null | undefined) {
  if (!value) {
    return "just now";
  }

  const deltaSeconds = Math.max(
    0,
    Math.round((Date.now() - new Date(value).getTime()) / 1000),
  );

  if (deltaSeconds < 60) {
    return `${deltaSeconds}s ago`;
  }

  const deltaMinutes = Math.round(deltaSeconds / 60);
  if (deltaMinutes < 60) {
    return `${deltaMinutes}m ago`;
  }

  const deltaHours = Math.round(deltaMinutes / 60);
  if (deltaHours < 24) {
    return `${deltaHours}h ago`;
  }

  return `${Math.round(deltaHours / 24)}d ago`;
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

function displayAddress(value: string | null | undefined) {
  if (!value) {
    return "-";
  }

  return value;
}

function navItemClasses(active: boolean) {
  return `group relative overflow-hidden rounded-2xl px-4 py-3 transition ${
    active
      ? "bg-white/[0.06] text-white shadow-[inset_0_0_0_1px_rgba(255,255,255,0.06)]"
      : "text-slate-300 hover:bg-white/[0.04] hover:text-white"
  }`;
}

function statusClasses(status: string) {
  if (status === "paid") {
    return "border border-emerald-400/18 bg-emerald-400/10 text-emerald-200";
  }
  if (status === "cancelled") {
    return "border border-rose-400/18 bg-rose-400/10 text-rose-200";
  }
  if (status === "expired") {
    return "border border-amber-400/18 bg-amber-400/10 text-amber-200";
  }

  return "border border-white/10 bg-white/[0.05] text-slate-300";
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
  const [createError, setCreateError] = useState("");
  const [activeSection, setActiveSection] =
    useState<(typeof SECTION_IDS)[number]>("overview");

  useEffect(() => {
    let cancelled = false;

    async function bootstrap() {
      setLoading(true);

      try {
        const [nextUser, nextInvoices] = await Promise.all([
          fetchCurrentUser(),
          fetchInvoices(),
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
  }, [router]);

  useEffect(() => {
    if (!user) {
      return;
    }

    const interval = window.setInterval(async () => {
      try {
        const nextInvoices = await fetchInvoices();
        setInvoices(nextInvoices);
      } catch (error) {
        if (error instanceof ApiError && error.status === 401) {
          router.replace("/auth?mode=sign-in");
        }
      }
    }, POLL_INTERVAL_MS);

    return () => window.clearInterval(interval);
  }, [router, user]);

  useEffect(() => {
    if (!modalOpen) {
      return;
    }

    const previousHtmlOverflow = document.documentElement.style.overflow;
    const previousHtmlOverscroll = document.documentElement.style.overscrollBehavior;
    const previousBodyOverflow = document.body.style.overflow;
    const previousBodyOverscroll = document.body.style.overscrollBehavior;

    document.documentElement.style.overflow = "hidden";
    document.documentElement.style.overscrollBehavior = "none";
    document.body.style.overflow = "hidden";
    document.body.style.overscrollBehavior = "none";

    return () => {
      document.documentElement.style.overflow = previousHtmlOverflow;
      document.documentElement.style.overscrollBehavior = previousHtmlOverscroll;
      document.body.style.overflow = previousBodyOverflow;
      document.body.style.overscrollBehavior = previousBodyOverscroll;
    };
  }, [modalOpen]);

  useEffect(() => {
    const sections = SECTION_IDS.map((id) => document.getElementById(id)).filter(
      (section): section is HTMLElement => Boolean(section),
    );

    if (sections.length === 0) {
      return;
    }

    const observer = new IntersectionObserver(
      (entries) => {
        const visible = entries
          .filter((entry) => entry.isIntersecting)
          .sort((left, right) => right.intersectionRatio - left.intersectionRatio)[0];

        if (visible?.target.id && SECTION_IDS.includes(visible.target.id as (typeof SECTION_IDS)[number])) {
          setActiveSection(visible.target.id as (typeof SECTION_IDS)[number]);
        }
      },
      {
        rootMargin: "-18% 0px -55% 0px",
        threshold: [0.15, 0.35, 0.6],
      },
    );

    sections.forEach((section) => observer.observe(section));
    return () => observer.disconnect();
  }, []);

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
    const expiredCount = invoices.filter((invoice) => invoice.status === "expired").length;
    const cancelledCount = invoices.filter((invoice) => invoice.status === "cancelled").length;
    const terminalCount = paidCount + expiredCount + cancelledCount;
    const successRate =
      terminalCount > 0 ? (paidCount / terminalCount) * 100 : paidCount > 0 ? 100 : 0;

    return {
      totalReceived,
      pendingTotal,
      paidCount,
      pendingCount,
      expiredCount,
      cancelledCount,
      successRate,
    };
  }, [invoices]);

  const recentActivity = useMemo<ActivityItem[]>(() => {
    const activity = [...invoices]
      .map((invoice) => {
        if (invoice.status === "paid" && invoice.paid_at) {
          return {
            id: `${invoice.id}-paid`,
            title: `${formatMoney(invoice.paid_amount_usdc || invoice.amount_usdc)} received`,
            detail:
              invoice.description ||
              invoice.client_email ||
              `Invoice ${invoice.id.slice(0, 8).toUpperCase()}`,
            time: formatRelativeTime(invoice.paid_at),
            tone: "success" as const,
            sortValue: new Date(invoice.paid_at).getTime(),
          };
        }

        if (invoice.status === "cancelled" || invoice.status === "expired") {
          return {
            id: `${invoice.id}-${invoice.status}`,
            title: invoice.status === "cancelled" ? "Invoice cancelled" : "Invoice expired",
            detail:
              invoice.description ||
              invoice.client_email ||
              `Invoice ${invoice.id.slice(0, 8).toUpperCase()}`,
            time: formatRelativeTime(invoice.created_at),
            tone: "neutral" as const,
            sortValue: new Date(invoice.created_at).getTime(),
          };
        }

        return {
          id: `${invoice.id}-pending`,
          title: "Invoice ready to share",
          detail:
            invoice.description ||
            invoice.client_email ||
            `Invoice ${invoice.id.slice(0, 8).toUpperCase()}`,
          time: formatRelativeTime(invoice.created_at),
          tone: "pending" as const,
          sortValue: new Date(invoice.created_at).getTime(),
        };
      })
      .sort((left, right) => right.sortValue - left.sortValue)
      .slice(0, 5);

    return activity.map((item) => ({
      id: item.id,
      title: item.title,
      detail: item.detail,
      time: item.time,
      tone: item.tone,
    }));
  }, [invoices]);

  const summaryAmount = useMemo(() => parseAmount(createState.amount_usdc), [createState.amount_usdc]);

  async function refreshInvoices() {
    setRefreshing(true);

    try {
      const nextInvoices = await fetchInvoices();
      setInvoices(nextInvoices);
      setNotice(null);
    } catch (error) {
      if (error instanceof ApiError && error.status === 401) {
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
    setCreateError("");
    setModalOpen(true);
  }

  function closeModal() {
    if (submitting) {
      return;
    }

    setModalOpen(false);
    setCreateState(initialInvoiceState);
    setCreateError("");
  }

  function updateField<K extends keyof CreateInvoiceState>(key: K, value: CreateInvoiceState[K]) {
    setCreateState((current) => ({ ...current, [key]: value }));
    setCreateError("");
    if (!submitting) {
      setCreateRequestId("");
    }
  }

  async function handleCreateInvoice(event: React.FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (submitting) {
      return;
    }

    setSubmitting(true);
    setNotice(null);
    setCreateError("");

    try {
      const clientRequestId = createRequestId || createClientRequestId();
      const invoice = await createInvoice({
        client_request_id: clientRequestId,
        amount_usdc: createState.amount_usdc,
        description: createState.description.trim(),
        client_email: createState.client_email.trim(),
        payout_address: createState.payout_address.trim(),
      });

      setInvoices((current) => {
        const withoutDuplicate = current.filter((existing) => existing.id !== invoice.id);
        return [invoice, ...withoutDuplicate];
      });
      setModalOpen(false);
      setCreateState(initialInvoiceState);
      setCreateRequestId("");
      setCreateError("");

      void refreshInvoices();

      const invoicePath = `/pay/${invoice.id}`;
      const copied = await copyInvoiceUrl(invoicePath);
      setNotice({
        type: "success",
        text: copied ? "Invoice link copied." : "Invoice created.",
        invoiceId: invoice.id,
      });
    } catch (error) {
      if (error instanceof ApiError && error.status === 401) {
        router.replace("/auth?mode=sign-in");
        return;
      }

      setCreateError(error instanceof Error ? error.message : "Unable to create invoice.");
    } finally {
      setSubmitting(false);
    }
  }

  async function handleSignOut() {
    try {
      const supabase = createSupabaseBrowserClient();
      await supabase.auth.signOut();
    } catch (error) {
      setNotice({
        type: "error",
        text: error instanceof Error ? error.message : "Unable to sign out.",
      });
      return;
    }

    router.replace("/");
    router.refresh();
  }

  async function handleCancelInvoice(invoiceId: string) {
    if (cancellingInvoiceId || !window.confirm("Cancel this invoice?")) {
      return;
    }

    setCancellingInvoiceId(invoiceId);
    setNotice(null);

    try {
      const cancelledInvoice = await cancelInvoice(invoiceId);
      const nextInvoices = await fetchInvoices();
      setInvoices(nextInvoices);
      setNotice({
        type: "success",
        text: `Invoice ${cancelledInvoice.id.slice(0, 8).toUpperCase()} cancelled.`,
      });
    } catch (error) {
      if (error instanceof ApiError && error.status === 401) {
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
      <main className="relative flex min-h-screen items-center justify-center overflow-x-hidden px-6 py-10">
        <div className="pointer-events-none absolute inset-x-0 top-[-10rem] mx-auto h-[28rem] w-[min(92vw,56rem)] rounded-full bg-[radial-gradient(circle,rgba(90,141,255,0.16),rgba(77,223,143,0.08)_44%,transparent_74%)] blur-3xl" />
        <div className="rounded-[1.75rem] border border-white/8 bg-[linear-gradient(180deg,rgba(255,255,255,0.05),rgba(255,255,255,0.02))] px-8 py-6 text-sm text-slate-300 shadow-[0_28px_90px_rgba(0,0,0,0.28)] backdrop-blur-xl">
          Loading dashboard...
        </div>
      </main>
    );
  }

  return (
    <main className="relative min-h-[100svh] overflow-x-hidden px-4 py-4 sm:px-6 sm:py-6">
      <div className="pointer-events-none absolute inset-x-0 top-[-10rem] mx-auto h-[28rem] w-[min(92vw,64rem)] rounded-full bg-[radial-gradient(circle,rgba(90,141,255,0.16),rgba(77,223,143,0.08)_44%,transparent_74%)] blur-3xl" />
      <div className="pointer-events-none absolute bottom-[-10rem] left-1/2 h-[24rem] w-[24rem] -translate-x-1/2 rounded-full bg-[radial-gradient(circle,rgba(248,211,111,0.12),transparent_72%)] blur-3xl" />

      <div className="relative mx-auto grid max-w-7xl items-start gap-4 lg:grid-cols-[260px_minmax(0,1fr)]">
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

          <nav className="mt-10 grid gap-2 text-sm">
            <a href="#overview" className={navItemClasses(activeSection === "overview")}>
              <span
                className={`absolute inset-y-2 left-2 w-1 rounded-full bg-gradient-to-b from-[#5a8dff] to-[#4ddf8f] transition-all ${
                  activeSection === "overview"
                    ? "opacity-100"
                    : "opacity-0 group-hover:opacity-60"
                }`}
              />
              <span className="pl-3">Overview</span>
            </a>
            <a href="#invoices" className={navItemClasses(activeSection === "invoices")}>
              <span
                className={`absolute inset-y-2 left-2 w-1 rounded-full bg-gradient-to-b from-[#5a8dff] to-[#4ddf8f] transition-all ${
                  activeSection === "invoices"
                    ? "opacity-100"
                    : "opacity-0 group-hover:opacity-60"
                }`}
              />
              <span className="pl-3">Invoices</span>
            </a>
            <a href="#wallet" className={navItemClasses(activeSection === "wallet")}>
              <span
                className={`absolute inset-y-2 left-2 w-1 rounded-full bg-gradient-to-b from-[#5a8dff] to-[#4ddf8f] transition-all ${
                  activeSection === "wallet"
                    ? "opacity-100"
                    : "opacity-0 group-hover:opacity-60"
                }`}
              />
              <span className="pl-3">Wallet</span>
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
            className="scroll-mt-4 flex flex-col gap-4 border-b border-white/6 pb-6 lg:scroll-mt-6 lg:flex-row lg:items-center lg:justify-between"
          >
            <div className="max-w-2xl">
              <p className="font-mono text-[11px] uppercase tracking-[0.28em] text-slate-500">
                Overview
              </p>
              <h1 className="mt-3 text-3xl font-semibold tracking-[-0.05em] text-white sm:text-4xl">
                Turn a wallet into your payments stack.
              </h1>
              <p className="mt-4 max-w-2xl text-sm leading-7 text-slate-300">
                Create an invoice, send the link, and let Aurefly confirm USDC settlement directly to your wallet.
              </p>
              <div className="mt-5 inline-flex items-center gap-2 rounded-full border border-emerald-400/18 bg-emerald-400/8 px-4 py-2 text-sm text-emerald-100/90">
                <span className="h-2 w-2 rounded-full bg-emerald-300 shadow-[0_0_12px_rgba(77,223,143,0.75)]" />
                Non-custodial. Funds go directly to your wallet.
              </div>
            </div>

            <div className="w-full max-w-md rounded-[1.8rem] border border-white/8 bg-[linear-gradient(180deg,rgba(255,255,255,0.06),rgba(255,255,255,0.02))] p-4 sm:p-5">
              <div className="text-xs uppercase tracking-[0.22em] text-slate-500">
                Primary action
              </div>
              <div className="mt-3 text-lg font-semibold tracking-[-0.03em] text-white">
                {invoices.length === 0 ? "Create your first invoice" : "Create your next invoice"}
              </div>
              <p className="mt-2 text-sm leading-6 text-slate-400">
                Aurefly exists to get one thing done fast: get you paid.
              </p>
              <div className="mt-5 flex flex-col gap-3 sm:flex-row">
                <button
                  type="button"
                  onClick={openModal}
                  className="inline-flex h-12 flex-1 items-center justify-center rounded-full bg-[#4f86ff] px-5 text-sm font-semibold text-white shadow-[0_12px_30px_rgba(79,134,255,0.24)] transition hover:-translate-y-px hover:bg-[#6595ff]"
                >
                  Create invoice →
                </button>
                <button
                  type="button"
                  onClick={refreshInvoices}
                  className="inline-flex h-12 items-center justify-center rounded-full border border-white/10 bg-white/[0.03] px-5 text-sm font-medium text-slate-100 transition hover:border-white/20 hover:bg-white/[0.05]"
                >
                  {refreshing ? "Refreshing..." : "Refresh"}
                </button>
              </div>
            </div>
          </header>

          <section className="mt-6 grid gap-4 xl:grid-cols-3">
            <article className="rounded-[1.7rem] border border-emerald-400/14 bg-emerald-400/8 p-5">
              <div className="text-sm text-emerald-100/80">Total volume</div>
              <div className="mt-4 text-3xl font-semibold tracking-[-0.05em] text-white">
                {formatMoney(metrics.totalReceived)}
              </div>
              <div className="mt-2 text-sm text-emerald-100/70">USDC processed</div>
            </article>
            <article className="rounded-[1.7rem] border border-sky-400/14 bg-sky-400/8 p-5">
              <div className="text-sm text-sky-100/80">Success rate</div>
              <div className="mt-4 text-3xl font-semibold tracking-[-0.05em] text-white">
                {metrics.successRate.toFixed(0)}%
              </div>
              <div className="mt-2 text-sm text-sky-100/70">
                {metrics.paidCount} paid · {metrics.expiredCount + metrics.cancelledCount} closed
              </div>
            </article>
            <article className="rounded-[1.7rem] border border-white/8 bg-white/[0.03] p-5">
              <div className="text-sm text-slate-300">Open now</div>
              <div className="mt-4 text-3xl font-semibold tracking-[-0.05em] text-white">
                {formatMoney(metrics.pendingTotal)}
              </div>
              <div className="mt-2 text-sm text-slate-400">{metrics.pendingCount} live invoices</div>
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
                </div>
              ) : null}
            </section>
          ) : null}

          <div className="mt-6 grid gap-6 xl:grid-cols-[minmax(0,1fr)_320px]">
            <section
              id="invoices"
              className="scroll-mt-4 rounded-[1.8rem] border border-white/7 bg-white/[0.03] p-4 lg:scroll-mt-6 sm:p-5"
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
                    const paymentLabel =
                      invoice.status === "cancelled"
                        ? "Cancelled"
                        : invoice.status === "expired"
                          ? "Expired"
                          : paidAmount > 0
                            ? `${formatMoney(paidAmount)} paid`
                            : "Awaiting payment";

                    return (
                      <article
                        key={invoice.id}
                        className="grid gap-4 rounded-[1.4rem] border border-white/7 bg-[#0c1520]/80 p-4 transition hover:border-white/12 hover:bg-[#101926]/90 lg:grid-cols-[minmax(0,1fr)_140px_170px]"
                      >
                        <div>
                          <div className="flex flex-wrap items-center gap-3">
                            <div className="text-sm font-semibold text-white">
                              {invoice.description ||
                                invoice.client_email ||
                                `Invoice ${invoice.id.slice(0, 8).toUpperCase()}`}
                            </div>
                            <span className={`inline-flex rounded-full px-3 py-1 text-xs font-semibold ${statusClasses(invoice.status)}`}>
                              {invoice.status === "paid"
                                ? "Paid"
                                : invoice.status === "cancelled"
                                  ? "Cancelled"
                                  : invoice.status === "expired"
                                    ? "Expired"
                                    : "Pending"}
                            </span>
                          </div>
                          {invoice.client_email && invoice.description ? (
                            <div className="mt-2 text-sm text-slate-400">{invoice.client_email}</div>
                          ) : null}
                          <div className="mt-3 flex flex-wrap items-center gap-x-4 gap-y-1 text-sm text-slate-500">
                            <span>{formatShortDate(invoice.created_at)}</span>
                            <span>{paymentLabel}</span>
                            <span className="font-mono text-[11px] uppercase tracking-[0.22em] text-slate-600">
                              {invoice.id.slice(0, 8).toUpperCase()}
                            </span>
                          </div>
                          <details className="mt-4 rounded-[1.1rem] border border-white/6 bg-white/[0.025] p-3 open:bg-white/[0.04]">
                            <summary className="cursor-pointer list-none text-sm font-medium text-slate-300">
                              Advanced details
                            </summary>
                            <div className="mt-3 grid gap-3 text-xs">
                              <div>
                                <div className="text-[10px] uppercase tracking-[0.18em] text-slate-500">
                                  Payout input
                                </div>
                                <code className="mt-1 block break-all font-mono text-slate-200">
                                  {displayAddress(invoice.requested_payout_address)}
                                </code>
                              </div>
                              <div>
                                <div className="text-[10px] uppercase tracking-[0.18em] text-slate-500">
                                  Merchant wallet
                                </div>
                                <code className="mt-1 block break-all font-mono text-slate-200">
                                  {displayAddress(invoice.wallet_pubkey)}
                                </code>
                              </div>
                              <div>
                                <div className="text-[10px] uppercase tracking-[0.18em] text-slate-500">
                                  USDC settlement account
                                </div>
                                <code className="mt-1 block break-all font-mono text-slate-200">
                                  {displayAddress(invoice.usdc_ata)}
                                </code>
                              </div>
                            </div>
                          </details>
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

            <aside className="space-y-4">
              <section className="rounded-[1.8rem] border border-white/7 bg-white/[0.03] p-5">
                <div className="flex items-center justify-between">
                  <div>
                    <div className="text-xs uppercase tracking-[0.22em] text-slate-500">Live</div>
                    <h2 className="mt-2 text-xl font-semibold tracking-[-0.03em] text-white">
                      Recent activity
                    </h2>
                  </div>
                  <div className="inline-flex items-center gap-2 rounded-full border border-emerald-400/14 bg-emerald-400/8 px-3 py-1 text-xs text-emerald-100/90">
                    <span className="h-2 w-2 rounded-full bg-emerald-300 shadow-[0_0_10px_rgba(77,223,143,0.8)]" />
                    Live
                  </div>
                </div>
                <div className="mt-5 grid gap-3">
                  {recentActivity.length === 0 ? (
                    <div className="rounded-[1.2rem] border border-white/7 bg-[#0c1520]/70 p-4 text-sm text-slate-400">
                      Activity appears here as invoices are created, paid, cancelled, or expired.
                    </div>
                  ) : (
                    recentActivity.map((item) => (
                      <article
                        key={item.id}
                        className="rounded-[1.2rem] border border-white/7 bg-[#0c1520]/70 p-4"
                      >
                        <div className="flex items-start justify-between gap-3">
                          <div className="flex items-start gap-3">
                            <span
                              className={`mt-1 h-2.5 w-2.5 rounded-full ${
                                item.tone === "success"
                                  ? "bg-emerald-300 shadow-[0_0_12px_rgba(77,223,143,0.75)]"
                                  : item.tone === "pending"
                                    ? "bg-sky-300 shadow-[0_0_12px_rgba(90,141,255,0.65)]"
                                    : "bg-slate-500"
                              }`}
                            />
                            <div>
                              <div className="text-sm font-medium text-white">{item.title}</div>
                              <div className="mt-1 text-sm leading-6 text-slate-400">{item.detail}</div>
                            </div>
                          </div>
                          <div className="whitespace-nowrap text-xs text-slate-500">{item.time}</div>
                        </div>
                      </article>
                    ))
                  )}
                </div>
              </section>

              <section className="rounded-[1.8rem] border border-white/7 bg-white/[0.03] p-5">
                <div className="text-xs uppercase tracking-[0.22em] text-slate-500">Trust</div>
                <div className="mt-3 text-lg font-semibold tracking-[-0.03em] text-white">
                  Payments land in your wallet
                </div>
                <p className="mt-3 text-sm leading-7 text-slate-400">
                  Aurefly never holds funds. Customers pay your wallet, and settlement is confirmed on-chain.
                </p>
              </section>
            </aside>
          </div>

          <section
            id="wallet"
            className="mt-6 scroll-mt-4 flex flex-col gap-3 rounded-[1.8rem] border border-white/7 bg-white/[0.03] p-5 lg:scroll-mt-6 sm:flex-row sm:items-center sm:justify-between"
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
        <div className="fixed inset-0 z-50 overflow-y-auto bg-[#03070d]/88 px-4 py-3 backdrop-blur-md sm:px-6 sm:py-4">
          <div className="flex min-h-full items-center justify-center">
            <section className="flex w-full max-w-lg flex-col overflow-hidden rounded-[2rem] border border-white/8 bg-[linear-gradient(180deg,rgba(255,255,255,0.055),rgba(255,255,255,0.02))] shadow-[0_34px_100px_rgba(0,0,0,0.38)] backdrop-blur-2xl">
            <div className="flex items-start justify-between gap-4 border-b border-white/6 px-5 pb-4 pt-5 sm:px-6 sm:pb-5 sm:pt-6">
              <div>
                <div className="font-mono text-[11px] uppercase tracking-[0.26em] text-slate-500">
                  Create invoice
                </div>
                <h2 className="mt-2 text-[1.65rem] font-semibold tracking-[-0.04em] text-white">
                  New invoice
                </h2>
                <p className="mt-2 text-sm leading-6 text-slate-300">
                  Set the amount and send payout directly to your wallet.
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

            <form onSubmit={handleCreateInvoice} className="flex flex-1 flex-col">
              <div className="grid gap-4 px-5 py-4 sm:px-6 sm:py-5">
              <label className="grid gap-2 text-sm text-slate-300">
                <span>Amount (USDC)</span>
                <div className="flex h-[3.25rem] items-center rounded-[1.35rem] border border-white/8 bg-[#0d1520]/92 px-4">
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
                  className="h-11 rounded-2xl border border-white/8 bg-[#0d1520]/92 px-4 text-white outline-none transition placeholder:text-slate-500 focus:border-sky-300/40 focus:bg-[#111b28]"
                  placeholder="e.g. Brand design project — Phase 1"
                />
              </label>

              <label className="grid gap-2 text-sm text-slate-300">
                <span>Client email (optional)</span>
                <input
                  type="email"
                  value={createState.client_email}
                  onChange={(event) => updateField("client_email", event.target.value)}
                  className="h-11 rounded-2xl border border-white/8 bg-[#0d1520]/92 px-4 text-white outline-none transition placeholder:text-slate-500 focus:border-sky-300/40 focus:bg-[#111b28]"
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
                  className="h-11 rounded-2xl border border-white/8 bg-[#0d1520]/92 px-4 text-white outline-none transition placeholder:text-slate-500 focus:border-sky-300/40 focus:bg-[#111b28]"
                  placeholder="Solana wallet address"
                />
                <span className="text-xs leading-6 text-slate-500">
                  Paste your wallet address or mainnet USDC account. Aurefly will lock this invoice to your wallet-owned USDC account and never reroute payout.
                </span>
              </label>

              <div className="rounded-[1.5rem] border border-white/7 bg-white/[0.03] p-5">
                <div className="text-[11px] uppercase tracking-[0.22em] text-slate-500">
                  Invoice amount
                </div>
                <div className="mt-3 text-2xl font-semibold tracking-[-0.05em] text-white">
                  {formatMoney(summaryAmount)}
                </div>
                <p className="mt-3 text-sm leading-6 text-slate-400">
                  Customers pay this amount and settlement goes directly to your wallet.
                </p>
              </div>

              {createError ? (
                <div className="rounded-[1.3rem] border border-rose-400/18 bg-rose-400/8 px-4 py-3 text-sm leading-6 text-rose-100">
                  {createError}
                </div>
              ) : null}
              </div>

              <div className="grid gap-3 border-t border-white/6 px-5 py-4 sm:grid-cols-2 sm:px-6 sm:py-5">
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
