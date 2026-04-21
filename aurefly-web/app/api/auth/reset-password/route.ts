import { NextResponse } from "next/server";

import { checkAuthRateLimit } from "@/lib/auth-rate-limit";
import { createClient as createSupabaseServerClient } from "@/lib/supabase/server";

type ResetPasswordPayload = {
  email?: string;
};

export async function POST(request: Request) {
  try {
    const rateLimited = checkAuthRateLimit(request, "reset-password", {
      limit: 5,
      windowMs: 10 * 60_000,
    });
    if (rateLimited) {
      return rateLimited;
    }

    const payload = (await request.json()) as ResetPasswordPayload;
    const email = payload.email?.trim() || "";

    if (!email) {
      return NextResponse.json(
        { error: "Email is required." },
        { status: 400 },
      );
    }

    const supabase = await createSupabaseServerClient();
    const origin = new URL(request.url).origin;
    const { error } = await supabase.auth.resetPasswordForEmail(email, {
      redirectTo: `${origin}/auth/reset-password`,
    });

    if (error) {
      return NextResponse.json({ error: error.message }, { status: 400 });
    }

    return NextResponse.json({ ok: true });
  } catch {
    return NextResponse.json({ error: "Internal server error." }, { status: 500 });
  }
}
