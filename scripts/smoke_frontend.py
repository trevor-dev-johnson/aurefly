import json
import os
import sys
import time
import urllib.request


BASE_URL = os.environ.get("BASE_URL", "http://localhost:8080")


def request(path, method="GET", body=None, headers=None):
    payload = None if body is None else json.dumps(body).encode()
    request_headers = {} if headers is None else dict(headers)
    if payload is not None:
        request_headers.setdefault("Content-Type", "application/json")

    request = urllib.request.Request(
        f"{BASE_URL}{path}",
        data=payload,
        headers=request_headers,
        method=method,
    )

    with urllib.request.urlopen(request, timeout=20) as response:
        return response.status, response.headers, response.read()


def main():
    email = f"frontend-{int(time.time())}@example.com"

    _, _, body = request("/")
    app_shell = body.decode()

    _, _, body = request(
        "/api/v1/auth/sign-up",
        method="POST",
        body={
            "email": email,
            "password": "Password123!",
            "name": "Frontend Smoke",
        },
    )
    signup = json.loads(body)
    token = signup["token"]
    auth_headers = {"Authorization": f"Bearer {token}"}

    _, _, body = request(
        "/api/v1/auth/sign-in",
        method="POST",
        body={
            "email": email,
            "password": "Password123!",
        },
    )
    sign_in = json.loads(body)

    _, _, body = request("/api/v1/auth/me", headers=auth_headers)
    me = json.loads(body)

    _, _, body = request(
        "/api/v1/me/invoices",
        method="POST",
        body={
            "amount_usdc": "1.234",
            "description": "Design work for homepage",
            "client_email": "client@example.com",
        },
        headers=auth_headers,
    )
    invoice = json.loads(body)

    _, _, body = request("/api/v1/me/invoices", headers=auth_headers)
    invoices = json.loads(body)

    _, _, body = request(f"/api/v1/invoices/{invoice['id']}")
    public_invoice = json.loads(body)

    _, _, body = request(f"/pay/{invoice['id']}")
    public_page = body.decode()

    qr_status, qr_headers, body = request(f"/api/v1/public/invoices/{invoice['id']}/qr.svg")
    qr_svg = body.decode()

    summary = {
        "email": email,
        "root_page_has_copy": "Send an invoice." in app_shell and "Get paid in seconds." in app_shell,
        "sign_in_email": sign_in["user"]["email"],
        "me_email": me["email"],
        "invoice_id": invoice["id"],
        "invoice_reference_pubkey": invoice["reference_pubkey"],
        "invoice_subtotal_usdc": invoice["subtotal_usdc"],
        "invoice_platform_fee_usdc": invoice["platform_fee_usdc"],
        "invoice_platform_fee_bps": invoice["platform_fee_bps"],
        "invoice_status": invoice["status"],
        "invoice_amount_usdc": invoice["amount_usdc"],
        "invoice_description": invoice.get("description"),
        "invoice_client_email": invoice.get("client_email"),
        "invoice_paid_amount_usdc": invoice["paid_amount_usdc"],
        "payment_uri": invoice["payment_uri"],
        "payment_uri_has_reference": "&reference=" in invoice["payment_uri"],
        "invoice_list_count": len(invoices),
        "private_invoice_has_client_email": any(
            row["id"] == invoice["id"] and row.get("client_email") == "client@example.com" for row in invoices
        ),
        "public_invoice_description": public_invoice.get("description"),
        "public_invoice_has_client_email": "client_email" in public_invoice,
        "public_page_has_heading": "Pay with USDC (Solana)" in public_page and "Payments usually confirm in ~10-15 seconds." in public_page,
        "public_page_has_wallet_hint": "Pay using Phantom or any Solana wallet." in public_page,
        "qr_status": qr_status,
        "qr_content_type": qr_headers.get("Content-Type"),
        "qr_has_svg": "<svg" in qr_svg[:200],
    }

    json.dump(summary, sys.stdout, separators=(",", ":"))


if __name__ == "__main__":
    main()
