import json
import os
import sys
import urllib.error
import urllib.request
import uuid


BASE_URL = sys.argv[1] if len(sys.argv) > 1 else "http://localhost:8080/api/v1"
VALID_PAYOUT_ADDRESS = (
    sys.argv[2]
    if len(sys.argv) > 2
    else os.environ.get("PAYOUT_ADDRESS", "AbC2BEBTyK45VHyeFodk7HBmeTzJBUoBxAvbt8nTXEUy")
)
PASSWORD = "validation-smoke-password"


def authed_request(method: str, path: str, payload: dict | None = None, token: str | None = None):
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

    try:
        with urllib.request.urlopen(req) as response:
            return response.status, json.loads(response.read().decode("utf-8"))
    except urllib.error.HTTPError as error:
        payload = error.read().decode("utf-8")
        try:
            data = json.loads(payload)
        except json.JSONDecodeError:
            data = {"error": payload}
        return error.code, data


def main() -> None:
    email = f"invoice-validation+{uuid.uuid4().hex[:8]}@example.com"
    sign_up_status, auth = authed_request(
        "POST",
        "/auth/sign-up",
        {
            "email": email,
            "password": PASSWORD,
            "name": "Invoice Validation",
        },
    )
    if sign_up_status != 201:
        raise SystemExit(f"sign-up failed: {sign_up_status} {auth}")

    token = auth["token"]

    missing_status, missing_error = authed_request(
        "POST",
        "/me/invoices",
        {"amount_usdc": "10.00"},
        token,
    )
    invalid_status, invalid_error = authed_request(
        "POST",
        "/me/invoices",
        {
            "amount_usdc": "10.00",
            "payout_address": "not-a-pubkey",
        },
        token,
    )
    valid_status, valid_invoice = authed_request(
        "POST",
        "/me/invoices",
        {
            "amount_usdc": "10.00",
            "payout_address": VALID_PAYOUT_ADDRESS,
        },
        token,
    )

    summary = {
        "missing_payout_status": missing_status,
        "missing_payout_error": missing_error.get("error"),
        "invalid_payout_status": invalid_status,
        "invalid_payout_error": invalid_error.get("error"),
        "valid_payout_status": valid_status,
        "valid_payout_invoice_id": valid_invoice.get("id"),
        "valid_payout_usdc_ata": valid_invoice.get("usdc_ata"),
        "valid_payout_matches_input": valid_invoice.get("usdc_ata") == VALID_PAYOUT_ADDRESS,
    }

    if missing_status != 400 or summary["missing_payout_error"] != "payout_address is required":
        raise SystemExit(f"missing payout validation failed: {summary}")
    if invalid_status != 400 or summary["invalid_payout_error"] != "invalid payout_address":
        raise SystemExit(f"invalid payout validation failed: {summary}")
    if valid_status != 201 or not summary["valid_payout_matches_input"]:
        raise SystemExit(f"valid payout flow failed: {summary}")

    json.dump(summary, sys.stdout, separators=(",", ":"))


if __name__ == "__main__":
    main()
