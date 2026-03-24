const base = process.argv[2] || "http://localhost:8080/api/v1";
const attempts = Number(process.argv[3] || "11");
const clientIp = process.argv[4] || "203.0.113.10";
const email = `ratelimit+${Math.random().toString(16).slice(2, 10)}@example.com`;
const password = "password123";

async function jsonRequest(path, options = {}) {
  const response = await fetch(`${base}${path}`, options);
  const body = await response.text();

  let data = null;
  if (body) {
    try {
      data = JSON.parse(body);
    } catch {
      data = body;
    }
  }

  return { response, data };
}

async function main() {
  const forwardedHeaders = {
    "Content-Type": "application/json",
    "X-Forwarded-For": clientIp,
  };

  const signUp = await jsonRequest("/auth/sign-up", {
    method: "POST",
    headers: forwardedHeaders,
    body: JSON.stringify({
      email,
      password,
      name: "Rate Limit Test",
    }),
  });

  if (!signUp.response.ok) {
    throw new Error(`sign-up failed: ${signUp.response.status} ${JSON.stringify(signUp.data)}`);
  }

  const statuses = [];

  for (let index = 0; index < attempts; index += 1) {
    const result = await jsonRequest("/auth/sign-in", {
      method: "POST",
      headers: forwardedHeaders,
      body: JSON.stringify({ email, password }),
    });
    statuses.push(result.response.status);
  }

  const rateLimitedAt = statuses.findIndex((status) => status === 429);
  if (rateLimitedAt === -1) {
    throw new Error(`expected a 429 but only saw statuses: ${statuses.join(",")}`);
  }

  console.log(
    JSON.stringify(
      {
        client_ip: clientIp,
        statuses,
        first_rate_limited_attempt: rateLimitedAt + 1,
      },
      null,
      2,
    ),
  );
}

main().catch((error) => {
  console.error(error);
  process.exit(1);
});
