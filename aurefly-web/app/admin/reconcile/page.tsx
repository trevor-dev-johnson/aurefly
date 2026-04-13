import { redirect } from "next/navigation";

import { createClient as createSupabaseServerClient } from "@/lib/supabase/server";
import { ReconcileClient } from "./reconcile-client";

export const metadata = {
  title: "Aurefly Reconciliation",
};

export default async function ReconcilePage() {
  const supabase = await createSupabaseServerClient();
  const {
    data: { user },
  } = await supabase.auth.getUser();

  if (!user) {
    redirect("/auth?mode=sign-in");
  }

  return <ReconcileClient />;
}
