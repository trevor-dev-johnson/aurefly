import json
import os
import sys
import urllib.request
import uuid


BASE_URL = sys.argv[1] if len(sys.argv) > 1 else "http://localhost:8080/api/v1"
PAYOUT_ADDRESS = (
    sys.argv[2]
    if len(sys.argv) > 2
    else os.environ.get("PAYOUT_ADDRESS", "AbC2BEBTyK45VHyeFodk7HBmeTzJBUoBxAvbt8nTXEUy")
)
PASSWORD = "idempotency-smoke-password"


def authed_request(method: str, path: str, payload: dict | None = None, token: str | None = None):
    body = None
    headers = {}

    if payload is not None:
        body = json.dumps(payload).encode("utf-8")
        headers["Content-Type"] = "application/json"
    if token:
        headers["Authorization"] = f"Bearer {token}"

    request = urllib.request.Request(
        url=f"{BASE_URL}{path}",
        data=body,
        headers=headers,
        method=method,
    )

    with urllib.request.urlopen(request, timeout=20) as response:
        return json.loads(response.read().decode("utf-8"))


def main() -> None:
    email = f"idempotency+{uuid.uuid4().hex[:8]}@example.com"
    auth = authed_request(
        "POST",
        "/auth/sign-up",
        {
            "email": email,
            "password": PASSWORD,
        },
    )
    token = auth["token"]
    client_request_id = str(uuid.uuid4())
    payload = {
        "client_request_id": client_request_id,
        "amount_usdc": "12.34",
        "description": "Duplicate submit smoke",
        "payout_address": PAYOUT_ADDRESS,
    }

    first = authed_request("POST", "/me/invoices", payload, token)
    second = authed_request("POST", "/me/invoices", payload, token)
    invoices = authed_request("GET", "/me/invoices", None, token)

    matching_invoices = [invoice for invoice in invoices if invoice["id"] == first["id"]]
    summary = {
        "client_request_id": client_request_id,
        "first_invoice_id": first["id"],
        "second_invoice_id": second["id"],
        "same_invoice_returned": first["id"] == second["id"],
        "matching_invoice_count": len(matching_invoices),
    }

    if not summary["same_invoice_returned"]:
        raise SystemExit(f"idempotency failed to return the same invoice: {summary}")
    if summary["matching_invoice_count"] != 1:
        raise SystemExit(f"idempotency created duplicates: {summary}")

    json.dump(summary, sys.stdout, separators=(",", ":"))


if __name__ == "__main__":
    main()
