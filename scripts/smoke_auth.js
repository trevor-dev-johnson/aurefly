const base = process.argv[2] || "http://localhost:8080/api/v1";
const email = `auth+${Math.random().toString(16).slice(2, 10)}@example.com`;

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
  const { response: signUpResponse, data: session } = await jsonRequest("/auth/sign-up", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({
      email,
      password: "password123",
      name: "Auth Test",
    }),
  });

  if (!signUpResponse.ok || !session?.token) {
    throw new Error(`sign-up failed: ${signUpResponse.status} ${JSON.stringify(session)}`);
  }

  const headers = {
    Authorization: `Bearer ${session.token}`,
  };

  const { response: meResponse, data: me } = await jsonRequest("/auth/me", { headers });
  const { response: logoutResponse } = await jsonRequest("/auth/logout", {
    method: "POST",
    headers,
  });
  const { response: afterLogoutResponse, data: afterLogout } = await jsonRequest("/auth/me", {
    headers,
  });

  if (!meResponse.ok) {
    throw new Error(`me failed: ${meResponse.status} ${JSON.stringify(me)}`);
  }

  if (logoutResponse.status !== 204) {
    throw new Error(`logout failed: ${logoutResponse.status}`);
  }

  if (afterLogoutResponse.status !== 401) {
    throw new Error(
      `post-logout /auth/me expected 401 but got ${afterLogoutResponse.status}: ${JSON.stringify(afterLogout)}`,
    );
  }

  console.log(
    JSON.stringify(
      {
        email: me.email,
        logout_status: logoutResponse.status,
        post_logout_me_status: afterLogoutResponse.status,
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
