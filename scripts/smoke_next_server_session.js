const NEXT_BASE = process.env.NEXT_BASE || "http://localhost:3000";

function parseCookieHeader(response) {
  const values =
    typeof response.headers.getSetCookie === "function"
      ? response.headers.getSetCookie()
      : [];

  return values.map((value) => value.split(";")[0]).join("; ");
}

async function readJson(response) {
  try {
    return await response.json();
  } catch {
    return {};
  }
}

async function main() {
  const email = `codex-${Date.now()}@example.com`;
  const password = "Aurefly!23456";
  const signupResponse = await fetch(`${NEXT_BASE}/api/auth/sign-up`, {
    method: "POST",
    headers: {
      "Content-Type": "application/json",
    },
    body: JSON.stringify({
      email,
      password,
    }),
  });
  const signupBody = await readJson(signupResponse);
  const cookieHeader = parseCookieHeader(signupResponse);

  if (!signupResponse.ok) {
    throw new Error(
      `sign-up failed (${signupResponse.status}): ${signupBody.error || "unknown error"}`,
    );
  }

  if (!cookieHeader) {
    throw new Error("sign-up did not return a session cookie");
  }

  const authHeaders = { cookie: cookieHeader };

  const meResponse = await fetch(`${NEXT_BASE}/api/me`, {
    headers: authHeaders,
  });
  const meBody = await readJson(meResponse);
  if (!meResponse.ok) {
    throw new Error(
      `/api/me failed (${meResponse.status}): ${meBody.error || "unknown error"}`,
    );
  }

  const invoiceResponse = await fetch(`${NEXT_BASE}/api/invoices`, {
    method: "POST",
    headers: {
      "Content-Type": "application/json",
      ...authHeaders,
    },
    body: JSON.stringify({
      client_request_id: crypto.randomUUID(),
      amount_usdc: "0.01",
      description: "Server-side auth smoke",
      payout_address: "GMZJbkbzsZRuKuQifoNhvgjQswQmPnHjrsZGRZUEegww",
    }),
  });
  const invoiceBody = await readJson(invoiceResponse);
  if (!invoiceResponse.ok) {
    throw new Error(
      `/api/invoices failed (${invoiceResponse.status}): ${invoiceBody.error || "unknown error"}`,
    );
  }

  console.log(
    JSON.stringify({
      email,
      signup_status: signupResponse.status,
      me_status: meResponse.status,
      me_user_id: meBody.id || null,
      invoice_status: invoiceResponse.status,
      invoice_id: invoiceBody.id || null,
    }),
  );
}

main().catch((error) => {
  console.error(error);
  process.exit(1);
});
