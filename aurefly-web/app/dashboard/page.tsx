import { redirect } from "next/navigation";

import { createClient as createSupabaseServerClient } from "@/lib/supabase/server";
import { DashboardClient } from "./dashboard-client";

export const metadata = {
  title: "Aurefly Dashboard",
};

export default async function DashboardPage() {
  const supabase = await createSupabaseServerClient();
  const {
    data: { user },
  } = await supabase.auth.getUser();

  if (!user) {
    redirect("/auth?mode=sign-in");
  }

  return <DashboardClient />;
}
