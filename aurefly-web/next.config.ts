import path from "path";
import type { NextConfig } from "next";

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

const csp = [
  "default-src 'self'",
  `script-src ${unique([
    "'self'",
    "'unsafe-inline'",
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

const nextConfig: NextConfig = {
  poweredByHeader: false,
  turbopack: {
    root: path.join(__dirname),
  },
  async headers() {
    return [
      {
        source: "/:path*",
        headers: [
          {
            key: "Content-Security-Policy",
            value: csp,
          },
          {
            key: "Referrer-Policy",
            value: "strict-origin-when-cross-origin",
          },
          {
            key: "X-Content-Type-Options",
            value: "nosniff",
          },
          {
            key: "X-Frame-Options",
            value: "DENY",
          },
          {
            key: "Permissions-Policy",
            value: "camera=(), microphone=(), geolocation=(), payment=()",
          },
          {
            key: "Strict-Transport-Security",
            value: "max-age=31536000; includeSubDomains; preload",
          },
        ],
      },
    ];
  },
};

export default nextConfig;
