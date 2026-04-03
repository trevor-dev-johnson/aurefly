import { NextResponse } from "next/server";

import { createClient as createSupabaseServerClient } from "@/lib/supabase/server";

type AuthPayload = {
  email?: string;
  password?: string;
};

export async function POST(request: Request) {
  try {
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
    const { data, error } = await supabase.auth.signUp({
      email,
      password,
    });

    if (error) {
      return NextResponse.json({ error: error.message }, { status: 400 });
    }

    return NextResponse.json({
      ok: true,
      requiresEmailConfirmation: !data.session,
    });
  } catch {
    return NextResponse.json({ error: "Internal server error." }, { status: 500 });
  }
}
