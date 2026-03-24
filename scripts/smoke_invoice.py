import json
import sys
import urllib.request
import uuid


BASE_URL = sys.argv[1] if len(sys.argv) > 1 else "http://localhost:8080/api/v1"
AMOUNT_USDC = sys.argv[2] if len(sys.argv) > 2 else "49.99"
PAYOUT_ADDRESS = sys.argv[3] if len(sys.argv) > 3 else None
PASSWORD = "smoke-test-password"


def request(method: str, path: str, payload: dict | None = None) -> dict:
    return authed_request(method, path, payload, None)


def authed_request(method: str, path: str, payload: dict | None = None, token: str | None = None) -> dict:
    body = None
    headers = {}

    if payload is not None:
        body = json.dumps(payload).encode("utf-8")
        headers["Content-Type"] = "application/json"
    if token:
        headers["Authorization"] = f"Bearer {token}"

    req = urllib.request.Request(
        url=f"{BASE_URL}{path}",
        data=body,
        headers=headers,
        method=method,
    )

    with urllib.request.urlopen(req) as response:
        return json.loads(response.read().decode("utf-8"))


def main() -> None:
    email = f"merchant+{uuid.uuid4().hex[:8]}@example.com"
    auth = request(
        "POST",
        "/auth/sign-up",
        {
            "email": email,
            "password": PASSWORD,
            "name": "Test Merchant",
        },
    )
    user = auth["user"]
    token = auth["token"]
    invoice = authed_request(
        "POST",
        "/me/invoices",
        {
            "amount_usdc": AMOUNT_USDC,
            **({"payout_address": PAYOUT_ADDRESS} if PAYOUT_ADDRESS else {}),
        },
        token,
    )
    fetched_invoice = authed_request("GET", f"/me/invoices", None, token)

    print(
        json.dumps(
            {
                "user": user,
                "created_invoice": invoice,
                "fetched_invoice": next(
                    item for item in fetched_invoice if item["id"] == invoice["id"]
                ),
            },
            indent=2,
        )
    )


if __name__ == "__main__":
    main()
