"use client";

import Link from "next/link";
import { useEffect, useMemo, useState } from "react";
import { useRouter } from "next/navigation";

import {
  ApiError,
  fetchCurrentUser,
  fetchUnmatchedPaymentDetail,
  fetchUnmatchedPayments,
  formatMoney,
  linkUnmatchedPayment,
  retryUnmatchedPayment,
  shortAddress,
  type AuthenticatedUser,
  type UnmatchedPaymentDetail,
  type UnmatchedPaymentSummary,
  updateUnmatchedPaymentStatus,
} from "@/lib/aurefly-api";

type Filters = {
  q: string;
  signature: string;
  invoice_id: string;
  reference: string;
  wallet: string;
  amount_usdc: string;
  status: string;
  date_from: string;
  date_to: string;
};

const initialFilters: Filters = {
  q: "",
  signature: "",
  invoice_id: "",
  reference: "",
  wallet: "",
  amount_usdc: "",
  status: "",
  date_from: "",
  date_to: "",
};

const statusOptions = [
  { value: "reviewed", label: "Reviewed" },
  { value: "resolved", label: "Resolved" },
  { value: "ignored", label: "Ignored" },
  { value: "refunded_manually", label: "Refunded manually" },
  { value: "needs_investigation", label: "Needs investigation" },
];

function formatDateTime(value: string | null | undefined) {
  if (!value) {
    return "-";
  }

  return new Date(value).toLocaleString("en-US", {
    month: "short",
    day: "numeric",
    hour: "numeric",
    minute: "2-digit",
  });
}

function statusClasses(status: string) {
  switch (status) {
    case "resolved":
      return "border border-emerald-400/18 bg-emerald-400/10 text-emerald-200";
    case "ignored":
      return "border border-slate-400/18 bg-slate-400/10 text-slate-200";
    case "refunded_manually":
      return "border border-rose-400/18 bg-rose-400/10 text-rose-200";
    case "needs_investigation":
      return "border border-amber-400/18 bg-amber-400/10 text-amber-200";
    case "reviewed":
      return "border border-sky-400/18 bg-sky-400/10 text-sky-200";
    default:
      return "border border-white/10 bg-white/[0.05] text-slate-300";
  }
}

function toPrettyJson(value: unknown) {
  try {
    return JSON.stringify(value ?? {}, null, 2);
  } catch {
    return "{}";
  }
}

export function ReconcileClient() {
  const router = useRouter();
  const [user, setUser] = useState<AuthenticatedUser | null>(null);
  const [loadingUser, setLoadingUser] = useState(true);
  const [accessDenied, setAccessDenied] = useState(false);
  const [filters, setFilters] = useState<Filters>(initialFilters);
  const [items, setItems] = useState<UnmatchedPaymentSummary[]>([]);
  const [selectedId, setSelectedId] = useState("");
  const [detail, setDetail] = useState<UnmatchedPaymentDetail | null>(null);
  const [loadingList, setLoadingList] = useState(false);
  const [loadingDetail, setLoadingDetail] = useState(false);
  const [pageError, setPageError] = useState("");
  const [actionError, setActionError] = useState("");
  const [actionMessage, setActionMessage] = useState("");
  const [linkInvoiceId, setLinkInvoiceId] = useState("");
  const [actionNote, setActionNote] = useState("");
  const [statusSelection, setStatusSelection] = useState("reviewed");
  const [actionPending, setActionPending] = useState("");

  const filterQuery = useMemo(() => {
    const query = new URLSearchParams();
    for (const [key, value] of Object.entries(filters)) {
      const trimmed = value.trim();
      if (trimmed) {
        if (key === "date_from" || key === "date_to") {
          query.set(key, new Date(trimmed).toISOString());
        } else {
          query.set(key, trimmed);
        }
      }
    }

    query.set("limit", "100");
    return query;
  }, [filters]);

  useEffect(() => {
    let cancelled = false;

    async function bootstrap() {
      setLoadingUser(true);
      try {
        const currentUser = await fetchCurrentUser();
        if (cancelled) {
          return;
        }

        setUser(currentUser);
        if (!currentUser.is_admin) {
          setAccessDenied(true);
          return;
        }
      } catch (error) {
        if (cancelled) {
          return;
        }

        if (error instanceof ApiError && error.status === 401) {
          router.replace("/auth?mode=sign-in");
          return;
        }

        setPageError(error instanceof Error ? error.message : "Unable to load admin access.");
      } finally {
        if (!cancelled) {
          setLoadingUser(false);
        }
      }
    }

    void bootstrap();
    return () => {
      cancelled = true;
    };
  }, [router]);

  async function loadList(query = filterQuery) {
    setLoadingList(true);
    setPageError("");

    try {
      const nextItems = await fetchUnmatchedPayments(query);
      setItems(nextItems);

      setSelectedId((current) => {
        if (current && nextItems.some((item) => item.id === current)) {
          return current;
        }

        return nextItems[0]?.id || "";
      });
    } catch (error) {
      if (error instanceof ApiError && error.status === 401) {
        router.replace("/auth?mode=sign-in");
        return;
      }
      if (error instanceof ApiError && error.status === 403) {
        setAccessDenied(true);
        return;
      }

      setPageError(error instanceof Error ? error.message : "Unable to load unmatched payments.");
    } finally {
      setLoadingList(false);
    }
  }

  useEffect(() => {
    if (loadingUser || accessDenied || !user?.is_admin) {
      return;
    }

    void loadList();
  }, [loadingUser, accessDenied, user?.is_admin, filterQuery]);

  useEffect(() => {
    if (!selectedId || accessDenied || !user?.is_admin) {
      setDetail(null);
      return;
    }

    let cancelled = false;

    async function loadDetail() {
      setLoadingDetail(true);
      setActionError("");
      setActionMessage("");

      try {
        const nextDetail = await fetchUnmatchedPaymentDetail(selectedId);
        if (cancelled) {
          return;
        }

        setDetail(nextDetail);
        setLinkInvoiceId(nextDetail.linked_invoice?.id || nextDetail.payment.linked_invoice_id || "");
        setActionNote(nextDetail.payment.notes || "");
      } catch (error) {
        if (cancelled) {
          return;
        }

        setActionError(error instanceof Error ? error.message : "Unable to load payment details.");
      } finally {
        if (!cancelled) {
          setLoadingDetail(false);
        }
      }
    }

    void loadDetail();
    return () => {
      cancelled = true;
    };
  }, [selectedId, accessDenied, user?.is_admin]);

  async function refreshAfterAction(nextDetail: UnmatchedPaymentDetail, successMessage: string) {
    setDetail(nextDetail);
    setActionMessage(successMessage);
    setActionError("");
    await loadList();
  }

  async function handleLinkInvoice() {
    if (!selectedId || !linkInvoiceId.trim()) {
      setActionError("Invoice ID is required to link a payment.");
      return;
    }

    setActionPending("link");
    setActionError("");
    setActionMessage("");

    try {
      const nextDetail = await linkUnmatchedPayment(selectedId, {
        invoice_id: linkInvoiceId.trim(),
        note: actionNote.trim() || undefined,
      });
      await refreshAfterAction(nextDetail, "Payment linked to invoice.");
    } catch (error) {
      setActionError(error instanceof Error ? error.message : "Unable to link payment.");
    } finally {
      setActionPending("");
    }
  }

  async function handleUpdateStatus() {
    if (!selectedId) {
      return;
    }

    setActionPending("status");
    setActionError("");
    setActionMessage("");

    try {
      const nextDetail = await updateUnmatchedPaymentStatus(selectedId, {
        status: statusSelection,
        note: actionNote.trim() || undefined,
      });
      await refreshAfterAction(nextDetail, "Status updated.");
    } catch (error) {
      setActionError(error instanceof Error ? error.message : "Unable to update status.");
    } finally {
      setActionPending("");
    }
  }

  async function handleRetryDetection() {
    if (!selectedId) {
      return;
    }

    setActionPending("retry");
    setActionError("");
    setActionMessage("");

    try {
      const nextDetail = await retryUnmatchedPayment(selectedId);
      await refreshAfterAction(nextDetail, "Retry detection completed.");
    } catch (error) {
      setActionError(error instanceof Error ? error.message : "Retry detection failed.");
    } finally {
      setActionPending("");
    }
  }

  if (loadingUser) {
    return (
      <main className="min-h-screen px-6 py-10 text-slate-200">
        <div className="mx-auto max-w-6xl rounded-[2rem] border border-white/8 bg-white/[0.03] p-8">
          Loading reconciliation...
        </div>
      </main>
    );
  }

  if (accessDenied) {
    return (
      <main className="min-h-screen px-6 py-10 text-slate-200">
        <div className="mx-auto max-w-3xl rounded-[2rem] border border-rose-400/18 bg-rose-400/8 p-8">
          <h1 className="text-2xl font-semibold text-white">Access denied</h1>
          <p className="mt-3 text-sm leading-7 text-rose-100/90">
            Your account is not on the Aurefly admin allowlist for reconciliation.
          </p>
          <Link href="/dashboard" className="mt-6 inline-flex text-sm font-medium text-white">
            Back to dashboard
          </Link>
        </div>
      </main>
    );
  }

  return (
    <main className="min-h-screen px-4 py-4 sm:px-6 sm:py-6">
      <div className="mx-auto max-w-7xl space-y-5">
        <header className="rounded-[2rem] border border-white/8 bg-white/[0.03] p-6 shadow-[0_24px_80px_rgba(0,0,0,0.24)] backdrop-blur-xl">
          <div className="flex flex-col gap-4 lg:flex-row lg:items-end lg:justify-between">
            <div>
              <div className="text-xs uppercase tracking-[0.22em] text-slate-500">Operations</div>
              <h1 className="mt-3 text-3xl font-semibold tracking-[-0.04em] text-white">
                Reconciliation
              </h1>
              <p className="mt-3 max-w-2xl text-sm leading-7 text-slate-400">
                Review unmatched payments, link them manually, retry detection, and leave an audit trail.
              </p>
            </div>
            <div className="flex flex-wrap gap-3">
              <Link
                href="/dashboard"
                className="inline-flex h-11 items-center justify-center rounded-full border border-white/10 bg-white/[0.03] px-5 text-sm font-medium text-slate-100 transition hover:bg-white/[0.05]"
              >
                Back to dashboard
              </Link>
              <button
                type="button"
                onClick={() => void loadList()}
                className="inline-flex h-11 items-center justify-center rounded-full bg-[#4f86ff] px-5 text-sm font-semibold text-white shadow-[0_12px_30px_rgba(79,134,255,0.24)] transition hover:bg-[#6595ff]"
              >
                Refresh
              </button>
            </div>
          </div>
        </header>

        <section className="rounded-[2rem] border border-white/8 bg-white/[0.03] p-5 shadow-[0_24px_80px_rgba(0,0,0,0.24)] backdrop-blur-xl">
          <div className="grid gap-3 md:grid-cols-2 xl:grid-cols-4">
            <input
              value={filters.q}
              onChange={(event) => setFilters((current) => ({ ...current, q: event.target.value }))}
              className="h-11 rounded-2xl border border-white/8 bg-[#0d1520]/92 px-4 text-sm text-white outline-none"
              placeholder="Search signature, wallet, amount, notes"
            />
            <input
              value={filters.signature}
              onChange={(event) => setFilters((current) => ({ ...current, signature: event.target.value }))}
              className="h-11 rounded-2xl border border-white/8 bg-[#0d1520]/92 px-4 text-sm text-white outline-none"
              placeholder="Signature"
            />
            <input
              value={filters.invoice_id}
              onChange={(event) => setFilters((current) => ({ ...current, invoice_id: event.target.value }))}
              className="h-11 rounded-2xl border border-white/8 bg-[#0d1520]/92 px-4 text-sm text-white outline-none"
              placeholder="Linked invoice ID"
            />
            <input
              value={filters.reference}
              onChange={(event) => setFilters((current) => ({ ...current, reference: event.target.value }))}
              className="h-11 rounded-2xl border border-white/8 bg-[#0d1520]/92 px-4 text-sm text-white outline-none"
              placeholder="Reference"
            />
            <input
              value={filters.wallet}
              onChange={(event) => setFilters((current) => ({ ...current, wallet: event.target.value }))}
              className="h-11 rounded-2xl border border-white/8 bg-[#0d1520]/92 px-4 text-sm text-white outline-none"
              placeholder="Destination or sender wallet"
            />
            <input
              value={filters.amount_usdc}
              onChange={(event) => setFilters((current) => ({ ...current, amount_usdc: event.target.value }))}
              className="h-11 rounded-2xl border border-white/8 bg-[#0d1520]/92 px-4 text-sm text-white outline-none"
              placeholder="Amount USDC"
            />
            <input
              type="datetime-local"
              value={filters.date_from}
              onChange={(event) => setFilters((current) => ({ ...current, date_from: event.target.value }))}
              className="h-11 rounded-2xl border border-white/8 bg-[#0d1520]/92 px-4 text-sm text-white outline-none"
            />
            <input
              type="datetime-local"
              value={filters.date_to}
              onChange={(event) => setFilters((current) => ({ ...current, date_to: event.target.value }))}
              className="h-11 rounded-2xl border border-white/8 bg-[#0d1520]/92 px-4 text-sm text-white outline-none"
            />
            <select
              value={filters.status}
              onChange={(event) => setFilters((current) => ({ ...current, status: event.target.value }))}
              className="h-11 rounded-2xl border border-white/8 bg-[#0d1520]/92 px-4 text-sm text-white outline-none"
            >
              <option value="">All statuses</option>
              <option value="pending">Pending</option>
              <option value="reviewed">Reviewed</option>
              <option value="resolved">Resolved</option>
              <option value="ignored">Ignored</option>
              <option value="refunded_manually">Refunded manually</option>
              <option value="needs_investigation">Needs investigation</option>
            </select>
            <button
              type="button"
              onClick={() => setFilters(initialFilters)}
              className="inline-flex h-11 items-center justify-center rounded-full border border-white/10 bg-white/[0.03] px-5 text-sm font-medium text-slate-100 transition hover:bg-white/[0.05]"
            >
              Clear filters
            </button>
          </div>
        </section>

        {pageError ? (
          <div className="rounded-[1.5rem] border border-rose-400/18 bg-rose-400/8 px-5 py-4 text-sm text-rose-100">
            {pageError}
          </div>
        ) : null}

        <section className="grid gap-5 xl:grid-cols-[420px_minmax(0,1fr)]">
          <div className="rounded-[2rem] border border-white/8 bg-white/[0.03] p-5 shadow-[0_24px_80px_rgba(0,0,0,0.24)] backdrop-blur-xl">
            <div className="flex items-center justify-between">
              <div>
                <div className="text-xs uppercase tracking-[0.22em] text-slate-500">Queue</div>
                <h2 className="mt-2 text-xl font-semibold tracking-[-0.03em] text-white">
                  Unmatched payments
                </h2>
              </div>
              <div className="rounded-full border border-white/8 bg-white/[0.04] px-3 py-1 text-xs text-slate-300">
                {items.length}
              </div>
            </div>

            <div className="mt-5 grid gap-3">
              {loadingList ? (
                <div className="rounded-[1.3rem] border border-white/7 bg-[#0c1520]/70 p-4 text-sm text-slate-400">
                  Loading unmatched payments...
                </div>
              ) : items.length === 0 ? (
                <div className="rounded-[1.3rem] border border-white/7 bg-[#0c1520]/70 p-4 text-sm text-slate-400">
                  No unmatched payments found for these filters.
                </div>
              ) : (
                items.map((item) => (
                  <button
                    key={item.id}
                    type="button"
                    onClick={() => setSelectedId(item.id)}
                    className={`rounded-[1.35rem] border p-4 text-left transition ${
                      selectedId === item.id
                        ? "border-sky-400/30 bg-sky-400/8"
                        : "border-white/7 bg-[#0c1520]/70 hover:border-white/12 hover:bg-[#111b28]"
                    }`}
                  >
                    <div className="flex items-start justify-between gap-3">
                      <div className="min-w-0">
                        <div className="text-sm font-medium text-white">
                          {shortAddress(item.signature)}
                        </div>
                        <div className="mt-1 text-sm text-slate-400">
                          {formatMoney(item.amount_usdc)} to {shortAddress(item.destination_wallet)}
                        </div>
                      </div>
                      <span className={`rounded-full px-3 py-1 text-[11px] font-semibold uppercase tracking-[0.18em] ${statusClasses(item.status)}`}>
                        {item.status.replaceAll("_", " ")}
                      </span>
                    </div>
                    <div className="mt-3 grid gap-1 text-xs text-slate-500">
                      <div>{item.reason.replaceAll("_", " ")}</div>
                      <div>{formatDateTime(item.seen_at)}</div>
                      {item.reference_pubkey ? <div>Ref {shortAddress(item.reference_pubkey)}</div> : null}
                    </div>
                  </button>
                ))
              )}
            </div>
          </div>

          <div className="rounded-[2rem] border border-white/8 bg-white/[0.03] p-5 shadow-[0_24px_80px_rgba(0,0,0,0.24)] backdrop-blur-xl">
            {!selectedId ? (
              <div className="rounded-[1.3rem] border border-white/7 bg-[#0c1520]/70 p-5 text-sm text-slate-400">
                Select an unmatched payment to inspect and reconcile it.
              </div>
            ) : loadingDetail ? (
              <div className="rounded-[1.3rem] border border-white/7 bg-[#0c1520]/70 p-5 text-sm text-slate-400">
                Loading payment details...
              </div>
            ) : detail ? (
              <div className="space-y-5">
                <div className="flex flex-col gap-3 border-b border-white/6 pb-5 lg:flex-row lg:items-start lg:justify-between">
                  <div>
                    <div className="text-xs uppercase tracking-[0.22em] text-slate-500">Payment detail</div>
                    <h2 className="mt-2 text-2xl font-semibold tracking-[-0.04em] text-white">
                      {shortAddress(detail.payment.signature)}
                    </h2>
                    <p className="mt-3 text-sm leading-7 text-slate-400">
                      Review the chain snapshot, operator notes, and audit history before changing state.
                    </p>
                  </div>
                  <span className={`rounded-full px-3 py-1 text-[11px] font-semibold uppercase tracking-[0.18em] ${statusClasses(detail.payment.status)}`}>
                    {detail.payment.status.replaceAll("_", " ")}
                  </span>
                </div>

                <div className="grid gap-4 xl:grid-cols-2">
                  <InfoBlock label="Amount" value={formatMoney(detail.payment.amount_usdc)} />
                  <InfoBlock label="Seen at" value={formatDateTime(detail.payment.seen_at)} />
                  <InfoBlock label="Destination wallet" value={detail.payment.destination_wallet} mono />
                  <InfoBlock label="Sender wallet" value={detail.payment.sender_wallet || "-"} mono />
                  <InfoBlock label="Reference" value={detail.payment.reference_pubkey || "-"} mono />
                  <InfoBlock label="Reason" value={detail.payment.reason.replaceAll("_", " ")} />
                </div>

                {detail.linked_invoice ? (
                  <section className="rounded-[1.5rem] border border-white/7 bg-[#0c1520]/70 p-5">
                    <div className="flex items-center justify-between gap-4">
                      <div>
                        <div className="text-xs uppercase tracking-[0.22em] text-slate-500">Linked invoice</div>
                        <div className="mt-2 text-lg font-semibold text-white">
                          {formatMoney(detail.linked_invoice.amount_usdc)}
                        </div>
                      </div>
                      <Link
                        href={`/pay/${detail.linked_invoice.id}`}
                        target="_blank"
                        className="text-sm font-medium text-sky-300 transition hover:text-sky-200"
                      >
                        Open invoice
                      </Link>
                    </div>
                    <div className="mt-4 grid gap-2 text-sm text-slate-300">
                      <div>Status: {detail.linked_invoice.status}</div>
                      <div>ID: <span className="font-mono text-xs">{detail.linked_invoice.id}</span></div>
                    </div>
                  </section>
                ) : null}

                {detail.existing_payment ? (
                  <section className="rounded-[1.5rem] border border-white/7 bg-[#0c1520]/70 p-5">
                    <div className="text-xs uppercase tracking-[0.22em] text-slate-500">Recorded payment</div>
                    <div className="mt-3 grid gap-2 text-sm text-slate-300">
                      <div>Invoice: <span className="font-mono text-xs">{detail.existing_payment.invoice_id}</span></div>
                      <div>Recipient: <span className="font-mono text-xs">{detail.existing_payment.recipient_token_account}</span></div>
                      <div>Finalized: {formatDateTime(detail.existing_payment.finalized_at)}</div>
                    </div>
                  </section>
                ) : null}

                <section className="rounded-[1.5rem] border border-white/7 bg-[#0c1520]/70 p-5">
                  <div className="text-xs uppercase tracking-[0.22em] text-slate-500">Operator actions</div>
                  <div className="mt-4 grid gap-3 xl:grid-cols-[minmax(0,1fr)_200px]">
                    <input
                      value={linkInvoiceId}
                      onChange={(event) => setLinkInvoiceId(event.target.value)}
                      className="h-11 rounded-2xl border border-white/8 bg-[#111b28] px-4 text-sm text-white outline-none"
                      placeholder="Invoice ID to link"
                    />
                    <button
                      type="button"
                      onClick={() => void handleLinkInvoice()}
                      disabled={actionPending === "link"}
                      className="inline-flex h-11 items-center justify-center rounded-full bg-[#4f86ff] px-5 text-sm font-semibold text-white transition disabled:opacity-70"
                    >
                      {actionPending === "link" ? "Linking..." : "Link to invoice"}
                    </button>
                  </div>

                  <div className="mt-3 grid gap-3 xl:grid-cols-[220px_minmax(0,1fr)_200px]">
                    <select
                      value={statusSelection}
                      onChange={(event) => setStatusSelection(event.target.value)}
                      className="h-11 rounded-2xl border border-white/8 bg-[#111b28] px-4 text-sm text-white outline-none"
                    >
                      {statusOptions.map((option) => (
                        <option key={option.value} value={option.value}>
                          {option.label}
                        </option>
                      ))}
                    </select>
                    <input
                      value={actionNote}
                      onChange={(event) => setActionNote(event.target.value)}
                      className="h-11 rounded-2xl border border-white/8 bg-[#111b28] px-4 text-sm text-white outline-none"
                      placeholder="Operator note"
                    />
                    <button
                      type="button"
                      onClick={() => void handleUpdateStatus()}
                      disabled={actionPending === "status"}
                      className="inline-flex h-11 items-center justify-center rounded-full border border-white/10 bg-white/[0.03] px-5 text-sm font-medium text-slate-100 transition hover:bg-white/[0.05] disabled:opacity-70"
                    >
                      {actionPending === "status" ? "Saving..." : "Update status"}
                    </button>
                  </div>

                  <button
                    type="button"
                    onClick={() => void handleRetryDetection()}
                    disabled={actionPending === "retry"}
                    className="mt-3 inline-flex h-11 items-center justify-center rounded-full border border-emerald-400/18 bg-emerald-400/10 px-5 text-sm font-medium text-emerald-100 transition hover:bg-emerald-400/14 disabled:opacity-70"
                  >
                    {actionPending === "retry" ? "Retrying..." : "Retry detection"}
                  </button>

                  {actionError ? (
                    <div className="mt-3 rounded-[1.2rem] border border-rose-400/18 bg-rose-400/8 px-4 py-3 text-sm text-rose-100">
                      {actionError}
                    </div>
                  ) : null}

                  {actionMessage ? (
                    <div className="mt-3 rounded-[1.2rem] border border-emerald-400/18 bg-emerald-400/8 px-4 py-3 text-sm text-emerald-100">
                      {actionMessage}
                    </div>
                  ) : null}
                </section>

                <section className="grid gap-5 xl:grid-cols-2">
                  <JsonBlock
                    label="Detector snapshot"
                    value={detail.metadata}
                    helper="Stored at the moment Aurefly marked this payment unmatched."
                  />
                  <JsonBlock
                    label="Chain snapshot"
                    value={detail.chain_snapshot || {}}
                    helper="Pulled from Solana for this signature when available."
                  />
                </section>

                <section className="rounded-[1.5rem] border border-white/7 bg-[#0c1520]/70 p-5">
                  <div className="text-xs uppercase tracking-[0.22em] text-slate-500">Audit trail</div>
                  <div className="mt-4 grid gap-3">
                    {detail.audit_events.length === 0 ? (
                      <div className="text-sm text-slate-400">No audit events recorded.</div>
                    ) : (
                      detail.audit_events.map((event) => (
                        <article
                          key={event.id}
                          className="rounded-[1.2rem] border border-white/7 bg-white/[0.025] p-4"
                        >
                          <div className="flex flex-col gap-2 lg:flex-row lg:items-center lg:justify-between">
                            <div className="text-sm font-medium text-white">
                              {event.action.replaceAll("_", " ")}
                            </div>
                            <div className="text-xs text-slate-500">{formatDateTime(event.created_at)}</div>
                          </div>
                          <div className="mt-2 text-sm text-slate-400">
                            {event.actor_email}
                            {event.previous_status || event.next_status
                              ? ` • ${event.previous_status || "-"} → ${event.next_status || "-"}`
                              : ""}
                          </div>
                          {event.note ? <div className="mt-2 text-sm text-slate-300">{event.note}</div> : null}
                          <pre className="mt-3 overflow-x-auto rounded-xl border border-white/7 bg-[#09111a] p-3 text-xs text-slate-300">
                            {toPrettyJson(event.metadata)}
                          </pre>
                        </article>
                      ))
                    )}
                  </div>
                </section>
              </div>
            ) : (
              <div className="rounded-[1.3rem] border border-white/7 bg-[#0c1520]/70 p-5 text-sm text-slate-400">
                Select an unmatched payment from the queue.
              </div>
            )}
          </div>
        </section>
      </div>
    </main>
  );
}

function InfoBlock({
  label,
  value,
  mono = false,
}: {
  label: string;
  value: string;
  mono?: boolean;
}) {
  return (
    <div className="rounded-[1.3rem] border border-white/7 bg-[#0c1520]/70 p-4">
      <div className="text-[11px] uppercase tracking-[0.2em] text-slate-500">{label}</div>
      <div className={`mt-3 text-sm text-white ${mono ? "break-all font-mono" : ""}`}>{value}</div>
    </div>
  );
}

function JsonBlock({
  label,
  value,
  helper,
}: {
  label: string;
  value: unknown;
  helper: string;
}) {
  return (
    <section className="rounded-[1.5rem] border border-white/7 bg-[#0c1520]/70 p-5">
      <div className="text-xs uppercase tracking-[0.22em] text-slate-500">{label}</div>
      <p className="mt-3 text-sm leading-6 text-slate-400">{helper}</p>
      <pre className="mt-4 overflow-x-auto rounded-[1.2rem] border border-white/7 bg-[#09111a] p-4 text-xs text-slate-300">
        {toPrettyJson(value)}
      </pre>
    </section>
  );
}
