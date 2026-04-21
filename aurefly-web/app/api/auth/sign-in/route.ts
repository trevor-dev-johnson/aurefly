import { NextResponse } from "next/server";

import { checkAuthRateLimit } from "@/lib/auth-rate-limit";
import { createClient as createSupabaseServerClient } from "@/lib/supabase/server";

type AuthPayload = {
  email?: string;
  password?: string;
};

export async function POST(request: Request) {
  try {
    const rateLimited = checkAuthRateLimit(request, "sign-in", {
      limit: 10,
      windowMs: 60_000,
    });
    if (rateLimited) {
      return rateLimited;
    }

    const payload = (await request.json()) as AuthPayload;
    const email = payload.email?.trim() || "";
    const password = payload.password || "";

    if (!email || !password) {
      return NextResponse.json(
        { error: "Email and password are required." },
        { status: 400 },
      );
    }

    const supabase = await createSupabaseServerClient();
    const { error } = await supabase.auth.signInWithPassword({
      email,
      password,
    });

    if (error) {
      return NextResponse.json({ error: error.message }, { status: 400 });
    }

    return NextResponse.json({ ok: true });
  } catch {
    return NextResponse.json({ error: "Internal server error." }, { status: 500 });
  }
}
