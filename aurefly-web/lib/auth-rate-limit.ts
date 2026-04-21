import { NextResponse } from "next/server";

type Bucket = {
  hits: number[];
};

type RateLimitOptions = {
  limit: number;
  windowMs: number;
};

const DEFAULT_OPTIONS: RateLimitOptions = {
  limit: 10,
  windowMs: 60_000,
};

const GLOBAL_STORE_KEY = "__aureflyAuthRateLimitStore";

function getStore() {
  const globalWithStore = globalThis as typeof globalThis & {
    [GLOBAL_STORE_KEY]?: Map<string, Bucket>;
  };

  if (!globalWithStore[GLOBAL_STORE_KEY]) {
    globalWithStore[GLOBAL_STORE_KEY] = new Map<string, Bucket>();
  }

  return globalWithStore[GLOBAL_STORE_KEY];
}

function getClientIp(request: Request) {
  const forwardedFor = request.headers.get("x-forwarded-for");
  if (forwardedFor) {
    const [first] = forwardedFor.split(",");
    const ip = first?.trim();
    if (ip) {
      return ip;
    }
  }

  const realIp = request.headers.get("x-real-ip")?.trim();
  if (realIp) {
    return realIp;
  }

  const connectingIp = request.headers.get("cf-connecting-ip")?.trim();
  if (connectingIp) {
    return connectingIp;
  }

  return "unknown";
}

function prune(bucket: Bucket, cutoff: number) {
  bucket.hits = bucket.hits.filter((timestamp) => timestamp > cutoff);
}

export function checkAuthRateLimit(
  request: Request,
  operation: string,
  options: Partial<RateLimitOptions> = {},
) {
  const { limit, windowMs } = { ...DEFAULT_OPTIONS, ...options };
  const now = Date.now();
  const cutoff = now - windowMs;
  const ip = getClientIp(request);
  const key = `${operation}:${ip}`;
  const store = getStore();
  const bucket = store.get(key) ?? { hits: [] };

  prune(bucket, cutoff);

  if (bucket.hits.length >= limit) {
    const oldestHit = bucket.hits[0] ?? now;
    const retryAfterSeconds = Math.max(
      1,
      Math.ceil((oldestHit + windowMs - now) / 1000),
    );

    return NextResponse.json(
      { error: "Too many attempts. Try again shortly." },
      {
        status: 429,
        headers: {
          "Retry-After": String(retryAfterSeconds),
        },
      },
    );
  }

  bucket.hits.push(now);
  store.set(key, bucket);

  if (store.size > 5_000) {
    for (const [entryKey, entryBucket] of store.entries()) {
      prune(entryBucket, cutoff);
      if (entryBucket.hits.length === 0) {
        store.delete(entryKey);
      }
    }
  }

  return null;
}
