import { createServerClient } from "@supabase/ssr";
import { NextResponse, type NextRequest } from "next/server";

import { getSupabasePublishableKey, getSupabaseUrl } from "./config";

function getOrigin(value: string | undefined) {
  if (!value) {
    return null;
  }

  try {
    return new URL(value).origin;
  } catch {
    return null;
  }
}

function unique(values: Array<string | null>) {
  return [...new Set(values.filter((value): value is string => Boolean(value)))];
}

function generateNonce() {
  const bytes = new Uint8Array(16);
  crypto.getRandomValues(bytes);
  let binary = "";

  for (const byte of bytes) {
    binary += String.fromCharCode(byte);
  }

  return btoa(binary);
}

function buildContentSecurityPolicy(nonce: string) {
  const isDev = process.env.NODE_ENV !== "production";
  const apiOrigin = getOrigin(process.env.NEXT_PUBLIC_API_URL);
  const supabaseOrigin = getOrigin(process.env.NEXT_PUBLIC_SUPABASE_URL);
  const websocketOrigins = unique([
    supabaseOrigin?.replace(/^https:/, "wss:") || null,
    apiOrigin?.replace(/^https:/, "wss:") || null,
    isDev ? "ws://localhost:3000" : null,
    isDev ? "ws://127.0.0.1:3000" : null,
    "wss://*.supabase.co",
  ]);

  return [
    "default-src 'self'",
    `script-src ${unique([
      "'self'",
      `'nonce-${nonce}'`,
      "'strict-dynamic'",
      isDev ? "'unsafe-eval'" : null,
    ]).join(" ")}`,
    `style-src ${unique(["'self'", "'unsafe-inline'"]).join(" ")}`,
    `img-src ${unique(["'self'", "data:", "blob:", apiOrigin]).join(" ")}`,
    `font-src ${unique(["'self'", "data:"]).join(" ")}`,
    `connect-src ${unique([
      "'self'",
      apiOrigin,
      supabaseOrigin,
      "https://*.supabase.co",
      isDev ? "http://localhost:3000" : null,
      isDev ? "http://127.0.0.1:3000" : null,
      ...websocketOrigins,
    ]).join(" ")}`,
    "object-src 'none'",
    "base-uri 'self'",
    "form-action 'self'",
    "frame-ancestors 'none'",
    "frame-src 'none'",
    "media-src 'none'",
    isDev ? null : "upgrade-insecure-requests",
  ]
    .filter(Boolean)
    .join("; ");
}

export async function refreshSupabaseSession(request: NextRequest) {
  const supabaseUrl = getSupabaseUrl();
  const supabasePublishableKey = getSupabasePublishableKey();
  const nonce = generateNonce();
  const requestHeaders = new Headers(request.headers);
  requestHeaders.set("x-nonce", nonce);

  if (!supabaseUrl || !supabasePublishableKey) {
    const response = NextResponse.next({
      request: {
        headers: requestHeaders,
      },
    });
    response.headers.set("Content-Security-Policy", buildContentSecurityPolicy(nonce));
    response.headers.set("x-nonce", nonce);
    return response;
  }

  let response = NextResponse.next({
    request: {
      headers: requestHeaders,
    },
  });

  const supabase = createServerClient(supabaseUrl, supabasePublishableKey, {
    cookies: {
      getAll() {
        return request.cookies.getAll();
      },
      setAll(cookiesToSet) {
        for (const { name, value } of cookiesToSet) {
          request.cookies.set(name, value);
        }

        response = NextResponse.next({
          request: {
            headers: requestHeaders,
          },
        });

        for (const { name, value, options } of cookiesToSet) {
          response.cookies.set(name, value, options);
        }
      },
    },
  });

  await supabase.auth.getUser();
  response.headers.set("Content-Security-Policy", buildContentSecurityPolicy(nonce));
  response.headers.set("x-nonce", nonce);
  return response;
}
