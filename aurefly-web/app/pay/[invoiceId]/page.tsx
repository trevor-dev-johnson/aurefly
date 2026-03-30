import Image from "next/image";
import Link from "next/link";
import { fetchPublicInvoice, type PublicInvoice } from "@/lib/aurefly-api";
import { PayInvoiceClient } from "./pay-invoice-client";

export const dynamic = "force-dynamic";

type PayPageProps = {
  params: Promise<{
    invoiceId: string;
  }>;
};

export default async function PayPage({ params }: PayPageProps) {
  const { invoiceId } = await params;
  let invoice: PublicInvoice | null = null;
  let message = "Unable to load invoice.";

  try {
    invoice = await fetchPublicInvoice(invoiceId);
  } catch (error) {
    message = error instanceof Error ? error.message : "Unable to load invoice.";
  }

  if (invoice) {
    return <PayInvoiceClient invoice={invoice} />;
  }

  return (
    <main className="relative flex min-h-screen items-center justify-center overflow-hidden px-6 py-10">
      <div className="pointer-events-none absolute inset-x-0 top-[-8rem] mx-auto h-[24rem] w-[min(90vw,54rem)] rounded-full bg-[radial-gradient(circle,rgba(90,141,255,0.14),rgba(77,223,143,0.06)_44%,transparent_74%)] blur-3xl" />
      <section className="relative w-full max-w-xl rounded-[2rem] border border-white/8 bg-[linear-gradient(180deg,rgba(255,255,255,0.055),rgba(255,255,255,0.02))] p-8 text-center shadow-[0_32px_100px_rgba(0,0,0,0.34)] backdrop-blur-2xl">
        <div className="mx-auto flex w-fit items-center gap-3">
          <Image
            src="/aurefly-logo.svg"
            alt="Aurefly"
            width={42}
            height={42}
            className="h-10 w-10 drop-shadow-[0_0_20px_rgba(248,211,111,0.2)]"
            priority
          />
          <span className="text-lg font-semibold tracking-[-0.03em] text-white">
            Aurefly
          </span>
        </div>

        <h1 className="mt-8 text-3xl font-semibold tracking-[-0.05em] text-white sm:text-4xl">
          Invoice unavailable
        </h1>
        <p className="mt-4 text-base leading-8 text-slate-300">{message}</p>

        <div className="mt-8 flex justify-center">
          <Link
            href="/"
            className="inline-flex h-12 items-center justify-center rounded-full bg-[#4f86ff] px-6 text-sm font-semibold text-white shadow-[0_12px_30px_rgba(79,134,255,0.24)] transition hover:-translate-y-px hover:bg-[#6595ff]"
          >
            Back to Aurefly
          </Link>
        </div>
      </section>
    </main>
  );
}
