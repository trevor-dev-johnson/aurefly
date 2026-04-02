"use client";
/* eslint-disable @next/next/no-img-element */

import Image from "next/image";
import Link from "next/link";
import { useEffect, useMemo, useRef, useState } from "react";
import {
  fetchPublicInvoice,
  formatMoney,
  getApiBase,
  invoiceHasRequiredReference,
  shortAddress,
  type PublicInvoice,
} from "@/lib/aurefly-api";

const DEFAULT_POLL_INTERVAL_MS = 10_000;
const FAST_POLL_INTERVAL_MS = 1_500;
const FAST_POLL_EXTENSION_MS = 60_000;
const RETURN_FROM_WALLET_POLL_MS = 30_000;

type PayInvoiceClientProps = {
  invoice: PublicInvoice;
};

export function PayInvoiceClient({ invoice }: PayInvoiceClientProps) {
  const [currentInvoice, setCurrentInvoice] = useState(invoice);
  const [awaitingWalletApproval, setAwaitingWalletApproval] = useState(false);
  const [copyLabel, setCopyLabel] = useState("Copy Payment Address");
  const fastPollUntilRef = useRef(0);
  const copyResetRef = useRef<number | null>(null);
  const apiBase = useMemo(() => getApiBase(), []);

  useEffect(() => {
    setCurrentInvoice(invoice);
  }, [invoice]);

  function extendFastPolling(durationMs: number) {
    fastPollUntilRef.current = Math.max(fastPollUntilRef.current, Date.now() + durationMs);
  }

  useEffect(() => {
    if (fastPollUntilRef.current === 0) {
      fastPollUntilRef.current = Date.now() + FAST_POLL_EXTENSION_MS;
    }
  }, []);

  useEffect(() => {
    if (currentInvoice.status !== "pending") {
      return;
    }

    let cancelled = false;
    let timer: number | undefined;

    const scheduleRefresh = (immediate = false) => {
      const delay = immediate
        ? 0
        : document.hidden
          ? DEFAULT_POLL_INTERVAL_MS
          : Date.now() < fastPollUntilRef.current
            ? FAST_POLL_INTERVAL_MS
            : DEFAULT_POLL_INTERVAL_MS;

      timer = window.setTimeout(() => {
        void refreshInvoice();
      }, delay);
    };

    const refreshInvoice = async () => {
      try {
        const nextInvoice = await fetchPublicInvoice(currentInvoice.id);
        if (cancelled) {
          return;
        }

        setCurrentInvoice(nextInvoice);

        if (nextInvoice.status === "pending") {
          scheduleRefresh();
        }
      } catch {
        if (!cancelled) {
          scheduleRefresh();
        }
      }
    };

    const handleVisibility = () => {
      if (document.hidden || currentInvoice.status !== "pending") {
        return;
      }

      extendFastPolling(RETURN_FROM_WALLET_POLL_MS);
      scheduleRefresh(true);
    };

    scheduleRefresh();
    document.addEventListener("visibilitychange", handleVisibility);

    return () => {
      cancelled = true;
      if (timer) {
        window.clearTimeout(timer);
      }
      document.removeEventListener("visibilitychange", handleVisibility);
    };
  }, [currentInvoice.id, currentInvoice.status]);

  useEffect(() => {
    return () => {
      if (copyResetRef.current !== null) {
        window.clearTimeout(copyResetRef.current);
      }
    };
  }, []);

  useEffect(() => {
    if (
      currentInvoice.status !== "pending" ||
      currentInvoice.payment_observed ||
      Number(currentInvoice.paid_amount_usdc || 0) > 0
    ) {
      setAwaitingWalletApproval(false);
    }
  }, [currentInvoice.paid_amount_usdc, currentInvoice.payment_observed, currentInvoice.status]);

  const paymentRouteReady = invoiceHasRequiredReference(currentInvoice);
  const paymentRecipient = getPaymentRecipient(
    currentInvoice.payment_uri,
    currentInvoice.wallet_pubkey || currentInvoice.usdc_ata,
  );
  const paidAmount = Number(currentInvoice.paid_amount_usdc || 0);
  const hasDetectedPayment = paidAmount > 0 && currentInvoice.status !== "paid";
  const hasObservedPayment =
    Boolean(currentInvoice.payment_observed) && currentInvoice.status !== "paid" && !hasDetectedPayment;
  const isAwaitingWalletApproval =
    awaitingWalletApproval &&
    !hasObservedPayment &&
    !hasDetectedPayment &&
    currentInvoice.status === "pending";
  const txUrl = currentInvoice.latest_payment_tx_url || currentInvoice.payment_observed_tx_url;

  const stateVariant =
    currentInvoice.status === "paid"
      ? "paid"
      : currentInvoice.status === "cancelled"
        ? "cancelled"
      : currentInvoice.status === "expired"
        ? "expired"
      : hasDetectedPayment
        ? "detected"
        : hasObservedPayment
          ? "confirming"
          : "waiting";

  const stateLabel =
    currentInvoice.status === "paid"
      ? "Payment complete"
      : currentInvoice.status === "cancelled"
        ? "Invoice cancelled"
      : currentInvoice.status === "expired"
        ? "Invoice expired"
      : hasDetectedPayment
        ? "Payment detected..."
        : hasObservedPayment
          ? "Transaction detected... confirming"
          : "Waiting for payment...";

  const statusText = !paymentRouteReady && currentInvoice.status === "pending"
    ? "This invoice is missing required payment routing metadata. Ask the merchant to regenerate it."
    : currentInvoice.status === "paid"
      ? `${formatMoney(currentInvoice.paid_amount_usdc)} received.`
      : currentInvoice.status === "cancelled"
        ? "This invoice is no longer accepting payment."
      : currentInvoice.status === "expired"
        ? "This invoice expired before payment was confirmed."
      : hasDetectedPayment
        ? `${formatMoney(currentInvoice.paid_amount_usdc)} received so far. Waiting for the full amount.`
        : hasObservedPayment
          ? "Transaction seen on Solana. Waiting for finalized confirmation."
          : isAwaitingWalletApproval
            ? "Open your wallet to approve the payment."
            : "Use the Aurefly payment link or QR. Manual transfers may not be credited automatically.";

  const statusDetail =
    currentInvoice.status === "paid"
      ? "Transaction confirmed on Solana."
      : currentInvoice.status === "cancelled"
        ? "Ask the merchant for a new invoice if you still need to pay."
        : currentInvoice.status === "expired"
          ? "Ask the merchant for a new invoice if you still need to pay."
        : null;
  function handlePayClick(event: React.MouseEvent<HTMLAnchorElement>) {
    if (currentInvoice.status !== "pending") {
      return;
    }

    event.preventDefault();
    if (!paymentRouteReady || !currentInvoice.payment_uri) {
      return;
    }

    setAwaitingWalletApproval(true);
    extendFastPolling(FAST_POLL_EXTENSION_MS);
    window.location.assign(currentInvoice.payment_uri);
  }

  async function handleCopyClick() {
    await navigator.clipboard.writeText(paymentRecipient);
    setCopyLabel(`Copied ✓ ${paymentRecipient.slice(-5)}`);

    if (copyResetRef.current !== null) {
      window.clearTimeout(copyResetRef.current);
    }

    copyResetRef.current = window.setTimeout(() => {
      setCopyLabel(currentInvoice.status === "paid" ? "Copied" : "Copy Payment Address");
    }, 1800);
  }

  return (
    <main className="relative min-h-[100svh] overflow-x-hidden px-4 py-4 sm:px-6 sm:py-5">
      <div className="pointer-events-none absolute inset-x-0 top-[-12rem] mx-auto h-[28rem] w-[min(94vw,62rem)] rounded-full bg-[radial-gradient(circle,rgba(90,141,255,0.16),rgba(77,223,143,0.08)_44%,transparent_74%)] blur-3xl" />
      <div className="pointer-events-none absolute inset-x-0 top-[22%] mx-auto h-[22rem] w-[min(92vw,38rem)] rounded-full bg-[radial-gradient(circle,rgba(248,211,111,0.11),transparent_74%)] blur-3xl" />

      <div className="relative mx-auto flex w-full max-w-5xl items-start justify-center lg:min-h-[calc(100svh-2.5rem)] lg:items-center">
        <section className="w-full overflow-hidden rounded-[1.75rem] border border-white/8 bg-[linear-gradient(180deg,rgba(255,255,255,0.055),rgba(255,255,255,0.02))] shadow-[0_32px_100px_rgba(0,0,0,0.34)] backdrop-blur-2xl">
          <div className="h-px w-full bg-[linear-gradient(90deg,transparent,rgba(77,223,143,0.4),transparent)]" />

          <div className="grid gap-6 px-4 py-4 sm:px-6 sm:py-6 lg:grid-cols-[minmax(0,1.05fr)_minmax(0,0.95fr)] lg:gap-8">
            <div className="flex min-w-0 flex-col justify-between gap-5">
              <header className="flex items-center justify-between gap-4 text-sm text-slate-300">
                <Link href="/" className="inline-flex items-center gap-3">
                  <Image
                    src="/aurefly-logo.svg"
                    alt="Aurefly"
                    width={38}
                    height={38}
                    className="h-9 w-9 drop-shadow-[0_0_18px_rgba(248,211,111,0.2)]"
                    priority
                  />
                  <span className="font-semibold tracking-[-0.03em] text-white">Aurefly</span>
                </Link>
                <span className="font-mono text-[11px] uppercase tracking-[0.24em] text-slate-500">
                  Live invoice
                </span>
              </header>

              <div className="text-center lg:text-left">
                <p className="text-[11px] uppercase tracking-[0.28em] text-slate-500">Amount due</p>
                <div className="mt-3 text-[clamp(2.8rem,8vw,4.9rem)] font-semibold tracking-[-0.08em] text-white">
                  {formatMoney(currentInvoice.amount_usdc)}
                </div>
                <div className="mt-3 inline-flex items-center rounded-full border border-emerald-400/15 bg-emerald-400/8 px-4 py-2 font-mono text-[11px] uppercase tracking-[0.22em] text-emerald-300">
                  USDC on Solana
                </div>
                {currentInvoice.description ? (
                  <p className="mt-4 max-w-xl text-sm leading-7 text-slate-300 sm:text-base">
                    {currentInvoice.description}
                  </p>
                ) : null}
              </div>

              <section
                className={`rounded-[1.4rem] border px-4 py-4 sm:px-5 sm:py-5 ${
                  stateVariant === "paid"
                    ? "border-emerald-400/18 bg-emerald-400/8"
                    : stateVariant === "expired"
                      ? "border-amber-400/18 bg-amber-400/10"
                    : stateVariant === "detected" || stateVariant === "confirming"
                      ? "border-sky-400/20 bg-sky-400/10"
                      : "border-white/8 bg-white/[0.04]"
                }`}
              >
                <div className="flex items-center gap-3">
                  {currentInvoice.status === "pending" ? (
                    <span className="inline-flex h-4 w-4 animate-spin rounded-full border-2 border-current border-r-transparent text-slate-300" />
                  ) : (
                    <span
                      className={`inline-flex h-4 w-4 items-center justify-center rounded-full text-[10px] ${
                        currentInvoice.status === "expired"
                          ? "bg-amber-400/18 text-amber-300"
                          : "bg-emerald-400/18 text-emerald-300"
                      }`}
                    >
                      {currentInvoice.status === "expired" ? "!" : "✓"}
                    </span>
                  )}
                  <strong className="text-base font-semibold text-white">{stateLabel}</strong>
                </div>
                <p className="mt-3 text-sm leading-7 text-slate-300">{statusText}</p>
                {statusDetail ? (
                  <p className="mt-2 text-sm leading-7 text-slate-400">{statusDetail}</p>
                ) : null}
                {txUrl ? (
                  <a
                    href={txUrl}
                    target="_blank"
                    rel="noreferrer"
                    className="mt-3 inline-flex text-sm font-medium text-sky-300 transition hover:text-sky-200"
                  >
                    {currentInvoice.status === "paid" ? "View on Explorer" : "View while confirming"}
                  </a>
                ) : null}
              </section>

              <p className="text-sm leading-7 text-slate-400">
                Payments usually confirm in ~10-15 seconds.
              </p>
            </div>

            <div className="flex min-w-0 flex-col justify-center gap-4">
              {currentInvoice.status === "pending" && paymentRouteReady ? (
                <a
                  href={currentInvoice.payment_uri || "#"}
                  onClick={handlePayClick}
                  className="inline-flex h-12 items-center justify-center rounded-full bg-[#4f86ff] px-6 text-sm font-semibold text-white shadow-[0_12px_24px_rgba(79,134,255,0.2)] transition hover:-translate-y-px hover:bg-[#6595ff]"
                >
                  Pay with Wallet
                </a>
              ) : null}

              {currentInvoice.status === "cancelled" || currentInvoice.status === "expired" ? (
                <div className="grid gap-3 rounded-[1.45rem] border border-white/6 bg-white/[0.025] p-5 text-left">
                  <span className="font-mono text-[11px] uppercase tracking-[0.26em] text-slate-500">
                    Invoice status
                  </span>
                  <p className="text-sm leading-7 text-slate-300">
                    {currentInvoice.status === "expired"
                      ? "This invoice expired and should no longer be paid."
                      : "This invoice has been cancelled by the merchant and should no longer be paid."}
                  </p>
                </div>
              ) : (
                <div className="grid gap-4 rounded-[1.45rem] border border-white/6 bg-white/[0.025] p-4 sm:p-5">
                  <div className="mx-auto w-full max-w-[220px] rounded-[1.2rem] bg-white p-3 shadow-[0_14px_30px_rgba(0,0,0,0.22)]">
                    <img
                      src={`${apiBase}/api/v1/public/invoices/${currentInvoice.id}/qr.svg`}
                      alt="Invoice QR code"
                      className="block h-auto w-full"
                    />
                  </div>

                  <div className="grid gap-3">
                    <div className="grid gap-2 text-left">
                      <span className="font-mono text-[11px] uppercase tracking-[0.26em] text-slate-500">
                        Merchant wallet
                      </span>
                      <code className="break-all rounded-2xl border border-white/8 bg-white/[0.03] px-4 py-3 font-mono text-sm text-white">
                        {shortAddress(paymentRecipient)}
                      </code>
                      <p className="text-xs leading-6 text-slate-400">
                        The QR and pay button both use this exact Solana Pay wallet recipient.
                      </p>
                    </div>

                    {currentInvoice.usdc_ata ? (
                      <div className="grid gap-2 text-left">
                        <span className="font-mono text-[11px] uppercase tracking-[0.26em] text-slate-500">
                          USDC settlement account
                        </span>
                        <code className="break-all rounded-2xl border border-white/8 bg-white/[0.03] px-4 py-3 font-mono text-sm text-white">
                          {shortAddress(currentInvoice.usdc_ata)}
                        </code>
                        <p className="text-xs leading-6 text-slate-400">
                          Your wallet routes the USDC transfer into this wallet-owned settlement account.
                        </p>
                      </div>
                    ) : null}
                  </div>

                  <button
                    type="button"
                    onClick={handleCopyClick}
                    className="inline-flex h-11 items-center justify-center rounded-full border border-white/10 bg-white/[0.03] px-6 text-sm font-medium text-slate-100 transition hover:border-white/20 hover:bg-white/[0.05]"
                  >
                    {copyLabel}
                  </button>

                  <p className="text-center text-sm leading-7 text-slate-400">
                    Use the Aurefly payment link or QR so your payment is credited automatically.
                  </p>
                </div>
              )}
            </div>
          </div>
        </section>
      </div>
    </main>
  );
}

function getPaymentRecipient(paymentUri: string | null | undefined, fallback: string) {
  if (!paymentUri) {
    return fallback;
  }

  const [recipient] = String(paymentUri).split("?");
  const cleaned = recipient.replace(/^solana:/, "").trim();
  return cleaned || fallback;
}
