import { NextResponse } from "next/server";

import { createClient as createSupabaseServerClient } from "@/lib/supabase/server";

export async function POST() {
  try {
    const supabase = await createSupabaseServerClient();
    const { error } = await supabase.auth.signOut();

    if (error) {
      return NextResponse.json({ error: error.message }, { status: 400 });
    }

    return new NextResponse(null, { status: 204 });
  } catch {
    return NextResponse.json({ error: "Internal server error." }, { status: 500 });
  }
}
