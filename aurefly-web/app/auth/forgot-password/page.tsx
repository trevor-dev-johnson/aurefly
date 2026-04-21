import { redirect } from "next/navigation";

import { createClient as createSupabaseServerClient } from "@/lib/supabase/server";
import { ForgotPasswordClient } from "./forgot-password-client";

export const metadata = {
  title: "Reset Password | Aurefly",
};

export default async function ForgotPasswordPage() {
  const supabase = await createSupabaseServerClient();
  const {
    data: { user },
  } = await supabase.auth.getUser();

  if (user) {
    redirect("/dashboard");
  }

  return <ForgotPasswordClient />;
}
