const API_BASE = process.env.API_BASE || "http://localhost:8080/api/v1";
const SUPABASE_URL =
  process.env.SUPABASE_URL || "https://wqptkvchxofjveolrwps.supabase.co";
const SUPABASE_PUBLISHABLE_KEY =
  process.env.SUPABASE_PUBLISHABLE_KEY ||
  "sb_publishable_J3S_Ho95ApVlN9eNqWcAAA_6yBUMbww";
const TEST_PAYOUT_ADDRESS =
  process.env.TEST_PAYOUT_ADDRESS ||
  "AbC2BEBTyK45VHyeFodk7HBmeTzJBUoBxAvbt8nTXEUy";
const TEST_PASSWORD = process.env.TEST_PASSWORD || "Aurefly!23456";
const TEST_EMAIL_DOMAIN = process.env.TEST_EMAIL_DOMAIN || "gmail.com";

async function fetchJson(url, options = {}) {
  const response = await fetch(url, options);
  const text = await response.text();
  let json = null;

  try {
    json = text ? JSON.parse(text) : null;
  } catch {
    json = text;
  }

  return { response, json };
}

async function signUpAndSignIn(email, password) {
  const headers = {
    apikey: SUPABASE_PUBLISHABLE_KEY,
    "Content-Type": "application/json",
  };
  const body = JSON.stringify({ email, password });

  const signup = await fetchJson(`${SUPABASE_URL}/auth/v1/signup`, {
    method: "POST",
    headers,
    body,
  });

  if (!signup.response.ok) {
    throw new Error(
      `Supabase sign-up failed (${signup.response.status}): ${JSON.stringify(signup.json)}`,
    );
  }

  if (signup.json?.session?.access_token) {
    return {
      email,
      supabaseUserId: signup.json.user?.id ?? null,
      accessToken: signup.json.session.access_token,
    };
  }

  const signin = await fetchJson(
    `${SUPABASE_URL}/auth/v1/token?grant_type=password`,
    {
      method: "POST",
      headers,
      body,
    },
  );

  if (!signin.response.ok || !signin.json?.access_token) {
    throw new Error(
      `Supabase sign-in failed (${signin.response.status}): ${JSON.stringify(signin.json)}`,
    );
  }

  return {
    email,
    supabaseUserId: signin.json.user?.id ?? signup.json?.user?.id ?? null,
    accessToken: signin.json.access_token,
  };
}

async function main() {
  const email = `codex.${Date.now()}@${TEST_EMAIL_DOMAIN}`;
  const { accessToken, supabaseUserId } = await signUpAndSignIn(email, TEST_PASSWORD);
  const authHeaders = {
    Authorization: `Bearer ${accessToken}`,
    "Content-Type": "application/json",
  };

  const me = await fetchJson(`${API_BASE}/auth/me`, {
    headers: authHeaders,
  });

  if (!me.response.ok) {
    throw new Error(
      `Backend /auth/me failed (${me.response.status}): ${JSON.stringify(me.json)}`,
    );
  }

  const createInvoice = await fetchJson(`${API_BASE}/me/invoices`, {
    method: "POST",
    headers: authHeaders,
    body: JSON.stringify({
      amount_usdc: "0.01",
      description: "Supabase bridge smoke",
      payout_address: TEST_PAYOUT_ADDRESS,
    }),
  });

  if (!createInvoice.response.ok) {
    throw new Error(
      `Backend invoice creation failed (${createInvoice.response.status}): ${JSON.stringify(
        createInvoice.json,
      )}`,
    );
  }

  console.log(
    JSON.stringify(
      {
        email,
        supabase_user_id: supabaseUserId,
        backend_user_id: me.json?.id ?? null,
        auth_bridge_ok: Boolean(supabaseUserId && me.json?.id === supabaseUserId),
        invoice_id: createInvoice.json?.id ?? null,
        invoice_status: createInvoice.json?.status ?? null,
      },
      null,
      2,
    ),
  );
}

main().catch((error) => {
  console.error(error.message || error);
  process.exit(1);
});
