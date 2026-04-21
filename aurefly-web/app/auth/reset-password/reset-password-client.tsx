"use client";

import Image from "next/image";
import Link from "next/link";
import { useRouter } from "next/navigation";
import { useEffect, useRef, useState } from "react";

import { createClient as createSupabaseBrowserClient } from "@/lib/supabase/browser";

export function ResetPasswordClient() {
  const router = useRouter();
  const [password, setPassword] = useState("");
  const [confirmPassword, setConfirmPassword] = useState("");
  const [status, setStatus] = useState("Checking your recovery session...");
  const [recoveryReady, setRecoveryReady] = useState(false);
  const [submitting, setSubmitting] = useState(false);
  const browserClientRef = useRef(createSupabaseBrowserClient());

  useEffect(() => {
    let cancelled = false;
    const supabase = browserClientRef.current;

    const finishReady = () => {
      if (cancelled) {
        return;
      }

      setRecoveryReady(true);
      setStatus("");
    };

    const finishMissingRecovery = () => {
      if (cancelled) {
        return;
      }

      setRecoveryReady(false);
      setStatus("Open the password reset link from your email to continue.");
    };

    const bootstrap = async () => {
      const { data, error } = await supabase.auth.getSession();

      if (cancelled) {
        return;
      }

      if (error) {
        setStatus(error.message);
        return;
      }

      if (data.session) {
        finishReady();
      } else {
        finishMissingRecovery();
      }
    };

    const {
      data: { subscription },
    } = supabase.auth.onAuthStateChange((event, session) => {
      if (cancelled) {
        return;
      }

      if (
        (event === "PASSWORD_RECOVERY" ||
          event === "SIGNED_IN" ||
          event === "TOKEN_REFRESHED") &&
        session
      ) {
        finishReady();
      }
    });

    void bootstrap();

    return () => {
      cancelled = true;
      subscription.unsubscribe();
    };
  }, []);

  async function handleSubmit(event: React.FormEvent<HTMLFormElement>) {
    event.preventDefault();

    if (!recoveryReady || submitting) {
      return;
    }

    if (password.length < 8) {
      setStatus("Password must be at least 8 characters.");
      return;
    }

    if (password !== confirmPassword) {
      setStatus("Passwords do not match.");
      return;
    }

    setSubmitting(true);
    setStatus("Updating password...");

    try {
      const supabase = browserClientRef.current;
      const { error } = await supabase.auth.updateUser({ password });

      if (error) {
        throw error;
      }

      setStatus("Password updated. Redirecting...");
      router.replace("/dashboard");
      router.refresh();
    } catch (error) {
      setStatus(
        error instanceof Error ? error.message : "Unable to update password.",
      );
      setSubmitting(false);
    }
  }

  return (
    <main className="relative flex min-h-screen items-center justify-center overflow-hidden px-6 py-10 sm:px-8">
      <div className="pointer-events-none absolute inset-x-0 top-[-10rem] mx-auto h-[28rem] w-[min(92vw,56rem)] rounded-full bg-[radial-gradient(circle,rgba(90,141,255,0.16),rgba(77,223,143,0.08)_44%,transparent_74%)] blur-3xl" />
      <div className="pointer-events-none absolute bottom-[-9rem] left-1/2 h-[24rem] w-[24rem] -translate-x-1/2 rounded-full bg-[radial-gradient(circle,rgba(248,211,111,0.12),transparent_72%)] blur-3xl" />

      <section className="relative w-full max-w-[28rem] rounded-[2rem] border border-white/8 bg-[linear-gradient(180deg,rgba(255,255,255,0.055),rgba(255,255,255,0.02))] p-7 shadow-[0_32px_100px_rgba(0,0,0,0.34)] backdrop-blur-2xl sm:p-8">
        <div className="h-px w-full bg-[linear-gradient(90deg,transparent,rgba(77,223,143,0.4),transparent)]" />

        <div className="mt-7 flex items-center justify-between gap-4">
          <Link href="/" className="inline-flex items-center gap-3">
            <Image
              src="/aurefly-logo.svg"
              alt="Aurefly"
              width={38}
              height={38}
              className="h-9 w-9 drop-shadow-[0_0_18px_rgba(248,211,111,0.2)]"
              priority
            />
            <span className="text-base font-semibold tracking-[-0.03em] text-white">
              Aurefly
            </span>
          </Link>
          <span className="rounded-full border border-white/8 bg-white/[0.03] px-3 py-1 text-[11px] uppercase tracking-[0.24em] text-slate-400">
            Recovery
          </span>
        </div>

        <div className="mt-10">
          <h1 className="text-3xl font-semibold tracking-[-0.05em] text-white">
            Choose a new password
          </h1>
          <p className="mt-4 text-sm leading-7 text-slate-300">
            This page only works from a valid email recovery link.
          </p>
        </div>

        <form onSubmit={handleSubmit} className="mt-8 grid gap-4">
          <label className="grid gap-2 text-sm text-slate-300">
            <span>New password</span>
            <input
              type="password"
              name="password"
              autoComplete="new-password"
              required
              minLength={8}
              value={password}
              onChange={(event) => setPassword(event.target.value)}
              className="h-12 rounded-2xl border border-white/8 bg-[#0d1520]/92 px-4 text-white outline-none transition placeholder:text-slate-500 focus:border-sky-300/40 focus:bg-[#111b28]"
              placeholder="Min 8 characters"
            />
          </label>

          <label className="grid gap-2 text-sm text-slate-300">
            <span>Confirm new password</span>
            <input
              type="password"
              name="confirmPassword"
              autoComplete="new-password"
              required
              minLength={8}
              value={confirmPassword}
              onChange={(event) => setConfirmPassword(event.target.value)}
              className="h-12 rounded-2xl border border-white/8 bg-[#0d1520]/92 px-4 text-white outline-none transition placeholder:text-slate-500 focus:border-sky-300/40 focus:bg-[#111b28]"
              placeholder="Repeat password"
            />
          </label>

          <button
            type="submit"
            disabled={!recoveryReady || submitting}
            className="mt-2 inline-flex h-12 items-center justify-center rounded-full bg-[#4f86ff] px-6 text-sm font-semibold text-white shadow-[0_12px_30px_rgba(79,134,255,0.24)] transition hover:-translate-y-px hover:bg-[#6595ff] disabled:cursor-not-allowed disabled:opacity-70"
          >
            {submitting ? "Updating..." : "Update password"}
          </button>
        </form>

        <p className="mt-4 min-h-6 text-sm text-slate-400">{status}</p>

        <div className="mt-6 flex items-center justify-between gap-4 border-t border-white/6 pt-5 text-sm text-slate-400">
          <p>
            Need a new link?{" "}
            <Link
              href="/auth/forgot-password"
              className="font-medium text-white transition hover:text-sky-200"
            >
              Send another
            </Link>
          </p>
          <Link href="/auth?mode=sign-in" className="transition hover:text-white">
            Sign in
          </Link>
        </div>
      </section>
    </main>
  );
}
