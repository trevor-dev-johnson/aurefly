"use client";

import Image from "next/image";
import Link from "next/link";
import { useEffect, useState } from "react";

type LandingHeaderProps = {
  demoInvoiceId: string;
};

const navLinks = [
  { href: "#how-it-works", label: "How it works" },
  { href: "#proof", label: "Why Aurefly" },
  { href: "#final-cta", label: "Start" },
];

export function LandingHeader({ demoInvoiceId }: LandingHeaderProps) {
  const [menuOpen, setMenuOpen] = useState(false);

  useEffect(() => {
    if (!menuOpen) {
      return;
    }

    const previousOverflow = document.body.style.overflow;
    const previousOverscroll = document.body.style.overscrollBehavior;

    document.body.style.overflow = "hidden";
    document.body.style.overscrollBehavior = "none";

    return () => {
      document.body.style.overflow = previousOverflow;
      document.body.style.overscrollBehavior = previousOverscroll;
    };
  }, [menuOpen]);

  useEffect(() => {
    if (!menuOpen) {
      return;
    }

    function handleKeyDown(event: KeyboardEvent) {
      if (event.key === "Escape") {
        setMenuOpen(false);
      }
    }

    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [menuOpen]);

  function closeMenu() {
    setMenuOpen(false);
  }

  return (
    <>
      <header className="relative z-30 rounded-full border border-white/8 bg-white/[0.03] px-5 py-3 backdrop-blur-xl">
        <div className="flex items-center justify-between gap-4">
          <Link href="/" className="inline-flex items-center gap-3">
            <Image
              src="/aurefly-logo.svg"
              alt="Aurefly"
              width={40}
              height={40}
              className="h-10 w-10 drop-shadow-[0_0_20px_rgba(248,211,111,0.2)]"
              priority
            />
            <span className="text-sm font-semibold tracking-[-0.03em] text-white">
              Aurefly
            </span>
          </Link>

          <nav className="hidden items-center gap-8 text-sm text-slate-300 lg:flex">
            {navLinks.map((item) => (
              <a key={item.href} href={item.href} className="transition hover:text-white">
                {item.label}
              </a>
            ))}
          </nav>

          <div className="hidden items-center gap-3 lg:flex">
            <Link
              href="/auth?mode=sign-in"
              className="inline-flex h-11 items-center justify-center rounded-full px-4 text-sm font-medium text-slate-300 transition hover:text-white"
            >
              Sign in
            </Link>
            <Link
              href={`/pay/${demoInvoiceId}`}
              className="inline-flex h-11 items-center justify-center rounded-full border border-white/10 px-5 text-sm font-medium text-slate-100 transition hover:border-white/20 hover:bg-white/[0.04]"
            >
              Try Demo Invoice
            </Link>
            <Link
              href="/auth?mode=sign-up"
              className="inline-flex h-11 items-center justify-center rounded-full bg-[#4f86ff] px-5 text-sm font-semibold text-white shadow-[0_10px_28px_rgba(79,134,255,0.24)] transition hover:-translate-y-px hover:bg-[#6595ff]"
            >
              Get Started
            </Link>
          </div>

          <button
            type="button"
            aria-expanded={menuOpen}
            aria-label={menuOpen ? "Close menu" : "Open menu"}
            onClick={() => setMenuOpen((open) => !open)}
            className="inline-flex h-11 w-11 items-center justify-center rounded-full border border-white/10 bg-white/[0.03] text-white transition hover:bg-white/[0.05] lg:hidden"
          >
            <div className="flex flex-col gap-1.5">
              <span
                className={`block h-px w-4 bg-current transition ${
                  menuOpen ? "translate-y-[7px] rotate-45" : ""
                }`}
              />
              <span
                className={`block h-px w-4 bg-current transition ${
                  menuOpen ? "opacity-0" : ""
                }`}
              />
              <span
                className={`block h-px w-4 bg-current transition ${
                  menuOpen ? "-translate-y-[7px] -rotate-45" : ""
                }`}
              />
            </div>
          </button>
        </div>
      </header>

      <div
        className={`fixed inset-0 z-40 bg-[linear-gradient(180deg,rgba(5,9,15,0.98),rgba(7,13,21,0.985))] transition duration-200 lg:hidden ${
          menuOpen ? "pointer-events-auto opacity-100" : "pointer-events-none opacity-0"
        }`}
      >
        <div className="pointer-events-none absolute inset-x-0 top-[-8rem] mx-auto h-[20rem] w-[20rem] rounded-full bg-[radial-gradient(circle,rgba(90,141,255,0.18),transparent_70%)] blur-3xl" />
        <div className="pointer-events-none absolute bottom-[-10rem] left-1/2 h-[18rem] w-[18rem] -translate-x-1/2 rounded-full bg-[radial-gradient(circle,rgba(77,223,143,0.12),transparent_72%)] blur-3xl" />

        <div className="relative flex min-h-screen flex-col px-6 pb-8 pt-6">
          <div className="flex items-center justify-between gap-4">
            <Link href="/" onClick={closeMenu} className="inline-flex items-center gap-3">
              <Image
                src="/aurefly-logo.svg"
                alt="Aurefly"
                width={40}
                height={40}
                className="h-10 w-10 drop-shadow-[0_0_20px_rgba(248,211,111,0.2)]"
              />
              <span className="text-sm font-semibold tracking-[-0.03em] text-white">
                Aurefly
              </span>
            </Link>

            <button
              type="button"
              onClick={closeMenu}
              aria-label="Close menu"
              className="inline-flex h-11 w-11 items-center justify-center rounded-full border border-white/10 bg-white/[0.04] text-white transition hover:bg-white/[0.07]"
            >
              <span className="text-lg leading-none">×</span>
            </button>
          </div>

          <div className="mt-10 overflow-hidden rounded-[2rem] border border-white/8 bg-[#09111b] shadow-[0_30px_80px_rgba(0,0,0,0.42)]">
            <div className="border-b border-white/6 px-5 py-4">
              <div className="text-[11px] uppercase tracking-[0.26em] text-slate-500">
                Menu
              </div>
              <div className="mt-3 max-w-[16rem] text-sm leading-6 text-slate-300">
                Create an invoice, share a link, and get paid directly to your wallet.
              </div>
            </div>

            <nav className="grid gap-1 p-3 text-base text-white">
              {navLinks.map((item) => (
                <a
                  key={item.href}
                  href={item.href}
                  onClick={closeMenu}
                  className="rounded-[1.2rem] px-4 py-4 transition hover:bg-white/[0.05]"
                >
                  {item.label}
                </a>
              ))}
            </nav>

            <div className="grid gap-3 border-t border-white/6 p-4">
              <Link
                href="/auth?mode=sign-in"
                onClick={closeMenu}
                className="inline-flex h-12 items-center justify-center rounded-full border border-white/10 bg-white/[0.03] text-sm font-medium text-white transition hover:bg-white/[0.05]"
              >
                Sign in
              </Link>
              <Link
                href={`/pay/${demoInvoiceId}`}
                onClick={closeMenu}
                className="inline-flex h-12 items-center justify-center rounded-full border border-white/10 bg-white/[0.03] text-sm font-medium text-white transition hover:bg-white/[0.05]"
              >
                Try Demo Invoice
              </Link>
              <Link
                href="/auth?mode=sign-up"
                onClick={closeMenu}
                className="inline-flex h-12 items-center justify-center rounded-full bg-[#4f86ff] text-sm font-semibold text-white shadow-[0_12px_28px_rgba(79,134,255,0.22)] transition hover:bg-[#6595ff]"
              >
                Get Started
              </Link>
            </div>
          </div>
        </div>
      </div>
    </>
  );
}
