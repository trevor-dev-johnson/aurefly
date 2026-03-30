import { createServerClient } from "@supabase/ssr";
import { cookies } from "next/headers";

import { assertSupabaseBrowserConfig } from "./config";

export async function createClient() {
  const cookieStore = await cookies();
  const { url, publishableKey } = assertSupabaseBrowserConfig();

  return createServerClient(url, publishableKey, {
    cookies: {
      getAll() {
        return cookieStore.getAll();
      },
      setAll(cookiesToSet) {
        try {
          for (const { name, value, options } of cookiesToSet) {
            cookieStore.set(name, value, options);
          }
        } catch {
          // Server Components cannot always write cookies; middleware handles refresh.
        }
      },
    },
  });
}

