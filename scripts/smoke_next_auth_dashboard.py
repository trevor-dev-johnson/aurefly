import json
import sys
import time
import urllib.request


API_BASE = sys.argv[1] if len(sys.argv) > 1 else "http://localhost:8080/api/v1"
APP_BASE = sys.argv[2] if len(sys.argv) > 2 else "http://localhost:3000"
PAYOUT_ADDRESS = (
    sys.argv[3] if len(sys.argv) > 3 else "AbC2BEBTyK45VHyeFodk7HBmeTzJBUoBxAvbt8nTXEUy"
)
PASSWORD = "next-flow-password"


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

    with urllib.request.urlopen(request, timeout=30) as response:
        payload = response.read().decode()
        return response.status, (json.loads(payload) if payload else None)


def request_status(url):
    with urllib.request.urlopen(url, timeout=30) as response:
        return response.status


def main():
    email = f"next-flow-{int(time.time())}@example.com"

    sign_up_status, sign_up = request_json(
        "POST",
        f"{API_BASE}/auth/sign-up",
        {"email": email, "password": PASSWORD},
    )
    sign_in_status, sign_in = request_json(
        "POST",
        f"{API_BASE}/auth/sign-in",
        {"email": email, "password": PASSWORD},
    )
    create_status, invoice = request_json(
        "POST",
        f"{API_BASE}/me/invoices",
        {
            "amount_usdc": "0.03",
            "payout_address": PAYOUT_ADDRESS,
            "description": "Next auth dashboard smoke",
        },
        token=sign_in["token"],
    )
    list_status, invoices = request_json(
        "GET",
        f"{API_BASE}/me/invoices",
        token=sign_in["token"],
    )

    result = {
        "email": email,
        "sign_up_status": sign_up_status,
        "sign_in_status": sign_in_status,
        "create_invoice_status": create_status,
        "list_invoices_status": list_status,
        "invoice_id": invoice["id"],
        "invoice_in_dashboard_data": any(item["id"] == invoice["id"] for item in invoices),
        "pay_page_status": request_status(f"{APP_BASE}/pay/{invoice['id']}"),
        "dashboard_page_status": request_status(f"{APP_BASE}/dashboard"),
        "auth_page_status": request_status(f"{APP_BASE}/auth"),
    }

    print(json.dumps(result, indent=2))


if __name__ == "__main__":
    main()
