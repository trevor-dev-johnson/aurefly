"use client";

import { createBrowserClient } from "@supabase/ssr";
import type { SupabaseClient } from "@supabase/supabase-js";

import { assertSupabaseBrowserConfig } from "./config";

let browserClient: SupabaseClient | null = null;

export function createClient() {
  if (browserClient) {
    return browserClient;
  }

  const { url, publishableKey } = assertSupabaseBrowserConfig();
  browserClient = createBrowserClient(url, publishableKey);
  return browserClient;
}

