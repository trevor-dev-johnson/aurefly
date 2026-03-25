"use client";
/* eslint-disable @next/next/no-img-element */

import Image from "next/image";
import Link from "next/link";
import { useRouter } from "next/navigation";
import { useEffect, useMemo, useRef, useState } from "react";
import {
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
  const router = useRouter();
  const [awaitingWalletApproval, setAwaitingWalletApproval] = useState(false);
  const [copyLabel, setCopyLabel] = useState("Copy Address");
  const fastPollUntilRef = useRef(0);
  const copyResetRef = useRef<number | null>(null);
  const apiBase = useMemo(() => getApiBase(), []);

  function extendFastPolling(durationMs: number) {
    fastPollUntilRef.current = Math.max(fastPollUntilRef.current, Date.now() + durationMs);
  }

  useEffect(() => {
    if (fastPollUntilRef.current === 0) {
      fastPollUntilRef.current = Date.now() + FAST_POLL_EXTENSION_MS;
    }
  }, []);

  useEffect(() => {
    if (invoice.status === "paid") {
      return;
    }

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
        router.refresh();
      }, delay);
    };

    const handleVisibility = () => {
      if (document.hidden || invoice.status === "paid") {
        return;
      }

      extendFastPolling(RETURN_FROM_WALLET_POLL_MS);
      scheduleRefresh(true);
    };

    scheduleRefresh();
    document.addEventListener("visibilitychange", handleVisibility);

    return () => {
      if (timer) {
        window.clearTimeout(timer);
      }
      document.removeEventListener("visibilitychange", handleVisibility);
    };
  }, [invoice.id, invoice.status, router]);

  useEffect(() => {
    return () => {
      if (copyResetRef.current) {
        window.clearTimeout(copyResetRef.current);
      }
    };
  }, []);

  const paymentRouteReady = invoiceHasRequiredReference(invoice);
  const paidAmount = Number(invoice.paid_amount_usdc || 0);
  const hasDetectedPayment = paidAmount > 0 && invoice.status !== "paid";
  const hasObservedPayment =
    Boolean(invoice.payment_observed) && invoice.status !== "paid" && !hasDetectedPayment;
  const isAwaitingWalletApproval =
    awaitingWalletApproval && !hasObservedPayment && !hasDetectedPayment && invoice.status !== "paid";
  const txUrl = invoice.latest_payment_tx_url || invoice.payment_observed_tx_url;

  const stateVariant =
    invoice.status === "paid"
      ? "paid"
      : hasDetectedPayment
        ? "detected"
        : hasObservedPayment
          ? "confirming"
          : "waiting";

  const stateLabel =
    invoice.status === "paid"
      ? "Payment complete"
      : hasDetectedPayment
        ? "Payment detected..."
        : hasObservedPayment
          ? "Transaction detected... confirming"
          : "Waiting for payment...";

  const statusText = !paymentRouteReady && invoice.status !== "paid"
    ? "This invoice is missing required payment routing metadata. Ask the merchant to regenerate it."
    : invoice.status === "paid"
      ? `${formatMoney(invoice.paid_amount_usdc)} received.`
      : hasDetectedPayment
        ? `${formatMoney(invoice.paid_amount_usdc)} received so far. Waiting for the full amount.`
        : hasObservedPayment
          ? "Transaction seen on Solana. Waiting for finalized confirmation."
          : isAwaitingWalletApproval
            ? "Open your wallet to approve the payment."
            : "Use the Aurefly payment link or QR. Manual transfers may not be credited automatically.";

  const statusDetail = invoice.status === "paid" ? "Transaction confirmed on Solana." : null;
  function handlePayClick(event: React.MouseEvent<HTMLAnchorElement>) {
    if (invoice.status === "paid") {
      return;
    }

    event.preventDefault();
    if (!paymentRouteReady || !invoice.payment_uri) {
      return;
    }

    setAwaitingWalletApproval(true);
    extendFastPolling(FAST_POLL_EXTENSION_MS);
    window.location.assign(invoice.payment_uri);
  }

  async function handleCopyClick() {
    await navigator.clipboard.writeText(invoice.usdc_ata);
    setCopyLabel(`Copied ✓ ${invoice.usdc_ata.slice(-5)}`);

    if (copyResetRef.current) {
      window.clearTimeout(copyResetRef.current);
    }

    copyResetRef.current = window.setTimeout(() => {
      setCopyLabel(invoice.status === "paid" ? "Copied" : "Copy Address");
    }, 1800);
  }

  return (
    <main className="relative min-h-screen overflow-hidden px-6 py-6 sm:px-8">
      <div className="pointer-events-none absolute inset-x-0 top-[-10rem] mx-auto h-[26rem] w-[min(92vw,60rem)] rounded-full bg-[radial-gradient(circle,rgba(90,141,255,0.16),rgba(77,223,143,0.08)_44%,transparent_74%)] blur-3xl" />
      <div className="pointer-events-none absolute inset-x-0 top-[15rem] mx-auto h-[20rem] w-[min(90vw,44rem)] rounded-full bg-[radial-gradient(circle,rgba(248,211,111,0.12),transparent_74%)] blur-3xl" />

      <div className="relative mx-auto flex min-h-[calc(100vh-3rem)] max-w-2xl flex-col items-center justify-center gap-8">
        <header className="flex w-full items-center justify-between gap-4 text-sm text-slate-300">
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
          <div className="inline-flex items-center gap-2 rounded-full border border-white/8 bg-white/[0.03] px-4 py-2 text-xs text-slate-300">
            <span className="h-2 w-2 rounded-full bg-emerald-400 shadow-[0_0_8px_rgba(52,214,123,0.75)]" />
            Secured by Solana
          </div>
        </header>

        <section className="w-full overflow-hidden rounded-[2rem] border border-white/8 bg-[linear-gradient(180deg,rgba(255,255,255,0.055),rgba(255,255,255,0.02))] shadow-[0_32px_100px_rgba(0,0,0,0.34)] backdrop-blur-2xl">
          <div className="h-px w-full bg-[linear-gradient(90deg,transparent,rgba(77,223,143,0.4),transparent)]" />

          <div className="flex items-start justify-between gap-4 border-b border-white/6 px-6 pb-5 pt-6 sm:px-8">
            <div>
              <p className="font-mono text-[11px] uppercase tracking-[0.28em] text-slate-500">
                Aurefly invoice
              </p>
              <h1 className="mt-3 text-base font-semibold tracking-[-0.03em] text-white">
                Ready to pay
              </h1>
            </div>
            <span className="inline-flex items-center gap-2 rounded-full border border-sky-400/15 bg-sky-400/8 px-3 py-1 text-xs font-semibold text-sky-200">
              <span className="h-2 w-2 rounded-full bg-sky-300" />
              Live
            </span>
          </div>

          <div className="border-b border-white/6 px-6 py-8 text-center sm:px-8 sm:py-10">
            <p className="text-[11px] uppercase tracking-[0.28em] text-slate-500">Amount due</p>
            <div className="mt-4 text-[clamp(3rem,10vw,4.8rem)] font-semibold tracking-[-0.08em] text-white">
              {formatMoney(invoice.amount_usdc)}
            </div>
            <div className="mt-3 inline-flex items-center rounded-full border border-emerald-400/15 bg-emerald-400/8 px-4 py-2 font-mono text-[11px] uppercase tracking-[0.22em] text-emerald-300">
              USDC on Solana
            </div>
            {invoice.description ? (
              <p className="mx-auto mt-5 max-w-lg text-sm leading-7 text-slate-300 sm:text-base">
                {invoice.description}
              </p>
            ) : null}
          </div>

          <div className="border-b border-white/6 px-6 py-5 sm:px-8">
            <div className="flex items-center justify-between gap-4 text-sm text-slate-300">
              <span>Subtotal</span>
              <span className="font-medium text-white">{formatMoney(invoice.subtotal_usdc ?? invoice.amount_usdc)}</span>
            </div>
            <div className="mt-3 flex items-center justify-between gap-4 text-sm text-slate-300">
              <span>Fee</span>
              <span className="font-medium text-white">
                {Number(invoice.platform_fee_usdc || 0) > 0
                  ? `Paid by merchant (${formatMoney(invoice.platform_fee_usdc)})`
                  : "No fee"}
              </span>
            </div>
          </div>

          <div className="px-6 pb-3 pt-6 text-center sm:px-8">
            <p className="text-sm leading-7 text-slate-300">
              Scan to pay or use the Aurefly payment link.
            </p>
          </div>

          {invoice.status !== "paid" && paymentRouteReady ? (
            <div className="grid gap-5 px-6 pb-4 pt-2 sm:px-8">
              <a
                href={invoice.payment_uri || "#"}
                onClick={handlePayClick}
                className="inline-flex h-13 items-center justify-center rounded-full bg-[#4f86ff] px-6 text-sm font-semibold text-white shadow-[0_12px_30px_rgba(79,134,255,0.24)] transition hover:-translate-y-px hover:bg-[#6595ff]"
              >
                Pay with Wallet
              </a>

              <div className="grid gap-5 rounded-[1.6rem] border border-white/6 bg-white/[0.025] p-5">
                <div className="mx-auto w-full max-w-[240px] rounded-[1.4rem] bg-white p-4 shadow-[0_14px_30px_rgba(0,0,0,0.22)]">
                  <img
                    src={`${apiBase}/api/v1/public/invoices/${invoice.id}/qr.svg`}
                    alt="Invoice QR code"
                    className="block h-auto w-full"
                  />
                </div>

                <div className="grid gap-2 text-left">
                  <span className="font-mono text-[11px] uppercase tracking-[0.26em] text-slate-500">
                    Wallet destination
                  </span>
                  <code className="rounded-2xl border border-white/8 bg-white/[0.03] px-4 py-4 font-mono text-sm text-white">
                    {shortAddress(invoice.usdc_ata)}
                  </code>
                </div>

                <button
                  type="button"
                  onClick={handleCopyClick}
                  className="inline-flex h-12 items-center justify-center rounded-full border border-white/10 bg-white/[0.03] px-6 text-sm font-medium text-slate-100 transition hover:border-white/20 hover:bg-white/[0.05]"
                >
                  {copyLabel}
                </button>

                <p className="text-center text-sm leading-7 text-slate-400">
                  Use the Aurefly payment link or QR so your payment is credited automatically.
                </p>
              </div>
            </div>
          ) : null}

          <p className="px-6 pt-4 text-center text-sm text-slate-400 sm:px-8">
            Payments usually confirm in ~10-15 seconds.
          </p>

          <div className="px-6 pb-6 pt-4 sm:px-8 sm:pb-8">
            <section
              className={`rounded-[1.5rem] border px-5 py-5 ${
                stateVariant === "paid"
                  ? "border-emerald-400/18 bg-emerald-400/8"
                  : stateVariant === "detected" || stateVariant === "confirming"
                    ? "border-sky-400/20 bg-sky-400/10"
                    : "border-white/8 bg-white/[0.04]"
              }`}
            >
              <div className="flex items-center gap-3">
                {invoice.status !== "paid" ? (
                  <span className="inline-flex h-4 w-4 animate-spin rounded-full border-2 border-current border-r-transparent text-slate-300" />
                ) : (
                  <span className="inline-flex h-4 w-4 items-center justify-center rounded-full bg-emerald-400/18 text-[10px] text-emerald-300">
                    ✓
                  </span>
                )}
                <strong className="text-base font-semibold text-white">{stateLabel}</strong>
              </div>
              <p className="mt-3 text-sm leading-7 text-slate-300">{statusText}</p>
              {statusDetail ? (
                <p className="mt-2 text-sm leading-7 text-slate-400">{statusDetail}</p>
              ) : null}
            </section>

            {txUrl ? (
              <a
                href={txUrl}
                target="_blank"
                rel="noreferrer"
                className="mt-4 inline-flex text-sm font-medium text-sky-300 transition hover:text-sky-200"
              >
                {invoice.status === "paid" ? "View on Explorer" : "View while confirming"}
              </a>
            ) : null}
          </div>
        </section>

        <p className="text-center text-sm text-slate-500">
          Powered by Aurefly · Non-custodial · Built on Solana
        </p>
      </div>
    </main>
  );
}
