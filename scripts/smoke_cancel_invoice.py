import json
import sys
import time
import urllib.error
import urllib.request


API_BASE = sys.argv[1] if len(sys.argv) > 1 else "http://localhost:8080/api/v1"
PAYOUT_ADDRESS = (
    sys.argv[2] if len(sys.argv) > 2 else "AbC2BEBTyK45VHyeFodk7HBmeTzJBUoBxAvbt8nTXEUy"
)
PASSWORD = "CancelPassword123!"


def request_json(method, url, body=None, token=None):
    headers = {"Content-Type": "application/json"} if body is not None else {}
    if token:
        headers["Authorization"] = f"Bearer {token}"

    request = urllib.request.Request(
        url,
        data=None if body is None else json.dumps(body).encode(),
        headers=headers,
        method=method,
    )

    try:
        with urllib.request.urlopen(request, timeout=30) as response:
            payload = response.read().decode()
            return response.status, (json.loads(payload) if payload else None)
    except urllib.error.HTTPError as error:
        payload = error.read().decode()
        data = json.loads(payload) if payload else None
        return error.code, data


def main():
    email = f"cancel-{int(time.time())}@example.com"

    sign_up_status, sign_up = request_json(
        "POST",
        f"{API_BASE}/auth/sign-up",
        {"email": email, "password": PASSWORD},
    )
    token = sign_up["token"]

    create_status, invoice = request_json(
        "POST",
        f"{API_BASE}/me/invoices",
        {
            "amount_usdc": "0.25",
            "description": "Cancel invoice smoke",
            "payout_address": PAYOUT_ADDRESS,
        },
        token=token,
    )

    if create_status != 201 or not isinstance(invoice, dict) or "id" not in invoice:
        raise SystemExit(
            json.dumps(
                {
                    "email": email,
                    "sign_up_status": sign_up_status,
                    "create_status": create_status,
                    "create_response": invoice,
                },
                indent=2,
            )
        )

    cancel_status, cancelled = request_json(
        "POST",
        f"{API_BASE}/me/invoices/{invoice['id']}/cancel",
        token=token,
    )

    list_status, invoices = request_json(
        "GET",
        f"{API_BASE}/me/invoices",
        token=token,
    )

    public_status, public_invoice = request_json(
        "GET",
        f"{API_BASE}/public/invoices/{invoice['id']}",
    )

    second_cancel_status, second_cancel = request_json(
        "POST",
        f"{API_BASE}/me/invoices/{invoice['id']}/cancel",
        token=token,
    )

    summary = {
        "email": email,
        "sign_up_status": sign_up_status,
        "create_status": create_status,
        "cancel_status": cancel_status,
        "list_status": list_status,
        "public_status": public_status,
        "second_cancel_status": second_cancel_status,
        "invoice_id": invoice["id"],
        "cancelled_status": cancelled["status"],
        "listed_status": next((row["status"] for row in invoices if row["id"] == invoice["id"]), None),
        "public_status_value": public_invoice["status"],
        "second_cancel_error": second_cancel.get("error") if isinstance(second_cancel, dict) else None,
    }

    if cancel_status != 200 or cancelled["status"] != "cancelled":
        raise SystemExit(f"cancel failed: {summary}")
    if summary["listed_status"] != "cancelled":
        raise SystemExit(f"cancel did not persist in private list: {summary}")
    if summary["public_status_value"] != "cancelled":
        raise SystemExit(f"cancel did not persist in public invoice: {summary}")
    if second_cancel_status != 400:
        raise SystemExit(f"second cancel should fail with 400: {summary}")

    print(json.dumps(summary, indent=2))


if __name__ == "__main__":
    main()
