import Link from "next/link";
import { LandingHeader } from "@/components/landing-header";

const DEMO_INVOICE_ID = "544ff0a7-3ee1-4d42-aa74-2305dc6921bf";

const steps = [
  {
    title: "Create invoice",
    detail: "Enter the amount in USDC and generate a payment page in seconds.",
  },
  {
    title: "Share link",
    detail: "Send one clean link or QR to your customer. No account needed to pay.",
  },
  {
    title: "Get paid",
    detail: "USDC settles directly to your wallet and Aurefly confirms automatically.",
  },
];

const proofPoints = [
  "Powered by Solana",
  "Non-custodial",
  "USDC only",
  "Payments settle in seconds",
];

export default function HomePage() {
  return (
    <main className="relative overflow-hidden">
      <div className="pointer-events-none absolute inset-x-0 top-[-12rem] mx-auto h-[28rem] w-[min(92vw,72rem)] rounded-full bg-[radial-gradient(circle,rgba(90,141,255,0.18),rgba(77,223,143,0.08)_44%,transparent_74%)] blur-3xl" />
      <div className="pointer-events-none absolute left-1/2 top-[18rem] h-[22rem] w-[22rem] -translate-x-1/2 rounded-full bg-[radial-gradient(circle,rgba(248,211,111,0.14),transparent_72%)] blur-3xl" />

      <div className="mx-auto flex min-h-screen max-w-6xl flex-col px-6 pb-24 pt-6 lg:px-8">
        <LandingHeader demoInvoiceId={DEMO_INVOICE_ID} />

        <section className="relative flex flex-1 flex-col items-center justify-center px-0 pb-18 pt-18 text-center sm:pt-24">
          <div className="max-w-3xl">
            <div className="inline-flex items-center gap-2 rounded-full border border-white/8 bg-white/[0.03] px-4 py-2 text-[11px] font-medium uppercase tracking-[0.26em] text-slate-300">
              <span className="h-2 w-2 rounded-full bg-[#4ddf8f] shadow-[0_0_10px_rgba(77,223,143,0.75)]" />
              Aurefly
            </div>

            <h1 className="balance mt-8 text-[clamp(3.3rem,9vw,6.4rem)] font-semibold tracking-[-0.08em] text-white">
              Get paid in USDC. Instantly.
            </h1>
            <p className="mx-auto mt-6 max-w-2xl text-base leading-8 text-slate-300 sm:text-lg">
              Create an invoice. Send a link. Funds hit your wallet directly, with no custody,
              no intermediaries, and no bank delays.
            </p>

            <div className="mt-10 flex flex-col items-center justify-center gap-3 sm:flex-row">
              <Link
                href="/auth?mode=sign-up"
                className="inline-flex h-12 min-w-[168px] items-center justify-center rounded-full bg-[#4f86ff] px-6 text-sm font-semibold text-white shadow-[0_12px_30px_rgba(79,134,255,0.24)] transition hover:-translate-y-px hover:bg-[#6595ff]"
              >
                Get Started
              </Link>
              <Link
                href={`/pay/${DEMO_INVOICE_ID}`}
                className="inline-flex h-12 min-w-[168px] items-center justify-center rounded-full border border-white/10 bg-white/[0.03] px-6 text-sm font-medium text-slate-100 transition hover:border-white/20 hover:bg-white/[0.05]"
              >
                Try Demo Invoice
              </Link>
            </div>
          </div>

          <div className="relative mt-16 w-full max-w-4xl">
            <div className="pointer-events-none absolute inset-x-8 inset-y-10 rounded-[2.5rem] bg-[radial-gradient(circle,rgba(90,141,255,0.18),rgba(77,223,143,0.08)_45%,transparent_76%)] blur-3xl" />
            <div className="relative mx-auto overflow-hidden rounded-[2rem] border border-white/8 bg-[linear-gradient(180deg,rgba(255,255,255,0.08),rgba(255,255,255,0.03))] p-6 text-left shadow-[0_32px_100px_rgba(0,0,0,0.34)] backdrop-blur-2xl sm:p-8">
              <div className="flex items-center justify-between gap-4">
                <div>
                  <p className="font-mono text-[11px] uppercase tracking-[0.28em] text-slate-400">
                    Aurefly invoice
                  </p>
                  <p className="mt-3 text-sm text-slate-400">Brand design for Acme Studio</p>
                </div>
                <span className="inline-flex items-center gap-2 rounded-full border border-emerald-400/15 bg-emerald-400/8 px-3 py-1 text-xs font-semibold text-emerald-300">
                  <span className="h-2 w-2 rounded-full bg-emerald-300 shadow-[0_0_10px_rgba(110,255,180,0.7)]" />
                  Paid
                </span>
              </div>

              <div className="mt-12 grid gap-8 lg:grid-cols-[1.1fr_0.9fr] lg:items-end">
                <div>
                  <div className="text-[11px] uppercase tracking-[0.28em] text-slate-500">
                    Amount
                  </div>
                  <div className="mt-4 text-[clamp(3.1rem,8vw,5rem)] font-semibold tracking-[-0.08em] text-white">
                    $2,400
                  </div>
                  <div className="mt-2 text-sm text-slate-400">USDC on Solana</div>

                  <div className="mt-10 grid gap-4 sm:grid-cols-3">
                    {[
                      ["Client", "Acme Corp"],
                      ["Reference", "Invoice #2847"],
                      ["Settlement", "Direct to wallet"],
                    ].map(([label, value]) => (
                      <div
                        key={label}
                        className="rounded-2xl border border-white/6 bg-white/[0.03] px-4 py-4"
                      >
                        <div className="text-xs text-slate-500">{label}</div>
                        <div className="mt-2 text-sm font-medium text-slate-100">{value}</div>
                      </div>
                    ))}
                  </div>
                </div>

                <div className="rounded-[1.6rem] border border-white/7 bg-[#0c1520]/84 p-5">
                  <div className="aspect-square rounded-[1.35rem] bg-[linear-gradient(135deg,#ffffff_0%,#eef2f7_100%)] p-5">
                    <div className="grid h-full grid-cols-8 gap-1.5 rounded-[1rem] bg-white p-4">
                      {Array.from({ length: 64 }).map((_, index) => {
                        const active = [
                          0, 1, 2, 3, 4, 6, 7, 8, 15, 16, 18, 19, 21, 23, 24, 25, 26, 28,
                          30, 32, 33, 35, 36, 39, 40, 41, 42, 44, 47, 48, 49, 50, 52, 55, 56,
                          57, 58, 60, 62, 63,
                        ].includes(index);

                        return (
                          <span
                            key={index}
                            className={`rounded-[3px] ${active ? "bg-slate-900" : "bg-slate-200"}`}
                          />
                        );
                      })}
                    </div>
                  </div>
                  <div className="mt-4 flex items-center justify-between rounded-2xl border border-emerald-400/15 bg-emerald-400/8 px-4 py-3">
                    <div>
                      <div className="text-xs uppercase tracking-[0.18em] text-emerald-300/80">
                        Status
                      </div>
                      <div className="mt-1 text-sm font-medium text-white">
                        Confirmed on Solana
                      </div>
                    </div>
                    <div className="text-right">
                      <div className="text-xs text-slate-400">Funds settled</div>
                      <div className="mt-1 text-sm font-semibold text-white">$2,400 USDC</div>
                    </div>
                  </div>
                </div>
              </div>
            </div>
          </div>
        </section>

        <section
          id="how-it-works"
          className="grid gap-6 border-t border-white/6 pt-24 sm:grid-cols-3"
        >
          {steps.map((step, index) => (
            <article
              key={step.title}
              className="rounded-[1.75rem] border border-white/6 bg-white/[0.025] p-6"
            >
              <div className="font-mono text-[11px] uppercase tracking-[0.26em] text-slate-500">
                0{index + 1}
              </div>
              <h2 className="mt-5 text-xl font-semibold tracking-[-0.04em] text-white">
                {step.title}
              </h2>
              <p className="mt-3 text-sm leading-7 text-slate-400">{step.detail}</p>
            </article>
          ))}
        </section>

        <section
          id="proof"
          className="grid gap-12 border-t border-white/6 pt-24 lg:grid-cols-[0.92fr_1.08fr] lg:items-center"
        >
          <div className="max-w-xl">
            <p className="font-mono text-[11px] uppercase tracking-[0.28em] text-slate-500">
              Why Aurefly
            </p>
            <h2 className="mt-5 text-4xl font-semibold tracking-[-0.06em] text-white sm:text-5xl">
              One link. One scan. Paid.
            </h2>
            <p className="mt-5 text-base leading-8 text-slate-300">
              Customers open the invoice, scan or tap pay, and send USDC from their
              wallet. Aurefly confirms automatically and settlement goes directly to the
              merchant.
            </p>

            <div className="mt-8 flex flex-wrap gap-3">
              {proofPoints.map((item) => (
                <span
                  key={item}
                  className="rounded-full border border-white/8 bg-white/[0.03] px-4 py-2 text-sm text-slate-200"
                >
                  {item}
                </span>
              ))}
            </div>
          </div>

          <div className="rounded-[2rem] border border-white/8 bg-[linear-gradient(180deg,rgba(255,255,255,0.05),rgba(255,255,255,0.03))] p-6 shadow-[0_28px_90px_rgba(0,0,0,0.28)] backdrop-blur-xl sm:p-8">
            <div className="rounded-[1.75rem] border border-emerald-400/18 bg-emerald-400/7 p-6">
              <div className="flex items-start justify-between gap-4">
                <div>
                  <p className="font-mono text-[11px] uppercase tracking-[0.28em] text-emerald-200/70">
                    Payment complete
                  </p>
                  <h3 className="mt-4 text-3xl font-semibold tracking-[-0.06em] text-white">
                    $1,246.34 received
                  </h3>
                </div>
                <span className="rounded-full border border-emerald-300/20 bg-emerald-300/10 px-3 py-1 text-xs font-semibold text-emerald-200">
                  Paid
                </span>
              </div>

              <div className="mt-10 grid gap-4 sm:grid-cols-3">
                <div className="rounded-2xl border border-white/6 bg-[#0b121c]/70 px-4 py-4">
                  <div className="text-xs text-slate-500">Network</div>
                  <div className="mt-2 text-sm font-medium text-white">Solana mainnet</div>
                </div>
                <div className="rounded-2xl border border-white/6 bg-[#0b121c]/70 px-4 py-4">
                  <div className="text-xs text-slate-500">Method</div>
                  <div className="mt-2 text-sm font-medium text-white">Solana Pay QR</div>
                </div>
                <div className="rounded-2xl border border-white/6 bg-[#0b121c]/70 px-4 py-4">
                  <div className="text-xs text-slate-500">Settlement</div>
                  <div className="mt-2 text-sm font-medium text-white">Direct to wallet</div>
                </div>
              </div>

              <p className="mt-6 text-sm leading-7 text-slate-300">
                No account needed to pay. Customers scan, approve, and Aurefly marks the
                invoice confirmed on Solana.
              </p>
            </div>
          </div>
        </section>

        <section id="final-cta" className="border-t border-white/6 pt-24">
          <div className="mx-auto max-w-3xl text-center">
            <p className="font-mono text-[11px] uppercase tracking-[0.28em] text-slate-500">
              Start today
            </p>
            <h2 className="mt-5 text-4xl font-semibold tracking-[-0.06em] text-white sm:text-5xl">
              Start accepting USDC in minutes.
            </h2>
            <p className="mx-auto mt-5 max-w-xl text-base leading-8 text-slate-300">
              Send a professional invoice, share one link, and let settlement land
              directly in your wallet.
            </p>
            <div className="mt-10 flex justify-center">
              <Link
                href="/auth?mode=sign-up"
                className="inline-flex h-12 items-center justify-center rounded-full bg-[#4f86ff] px-6 text-sm font-semibold text-white shadow-[0_12px_30px_rgba(79,134,255,0.24)] transition hover:-translate-y-px hover:bg-[#6595ff]"
              >
                Create your first invoice
              </Link>
            </div>
          </div>
        </section>
      </div>
    </main>
  );
}
