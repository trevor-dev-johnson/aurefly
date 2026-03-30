import "server-only";

import { createClient as createSupabaseServerClient } from "@/lib/supabase/server";

const API_PREFIX = "/api/v1";
const DEFAULT_API_URL = "http://localhost:8080";

export class BackendApiError extends Error {
  status: number;

  constructor(message: string, status: number) {
    super(message);
    this.name = "BackendApiError";
    this.status = status;
  }
}

export function getBackendApiBase() {
  return process.env.NEXT_PUBLIC_API_URL || DEFAULT_API_URL;
}

export async function requireSessionAccessToken() {
  const supabase = await createSupabaseServerClient();
  const {
    data: { session },
    error,
  } = await supabase.auth.getSession();

  if (error || !session?.access_token) {
    throw new BackendApiError("Unauthorized", 401);
  }

  return session.access_token;
}

type BackendFetchOptions = {
  method?: string;
  body?: unknown;
  accessToken?: string;
  headers?: HeadersInit;
};

export async function backendFetch<T>(path: string, options: BackendFetchOptions = {}) {
  const headers = new Headers(options.headers || {});

  if (options.body !== undefined && !headers.has("Content-Type")) {
    headers.set("Content-Type", "application/json");
  }

  if (options.accessToken) {
    headers.set("Authorization", `Bearer ${options.accessToken}`);
  }

  const response = await fetch(`${getBackendApiBase()}${API_PREFIX}${path}`, {
    method: options.method || "GET",
    headers,
    body: options.body === undefined ? undefined : JSON.stringify(options.body),
    cache: "no-store",
  });

  if (response.status === 204) {
    return undefined as T;
  }

  const payload = await response.json().catch(() => ({}));

  if (!response.ok) {
    throw new BackendApiError(
      typeof payload?.error === "string" ? payload.error : "Request failed.",
      response.status,
    );
  }

  return payload as T;
}

