import json
import os
import sys
import time
import urllib.request


API_BASE = os.environ.get("API_BASE", "http://localhost:8080")
APP_BASE = os.environ.get("APP_BASE", "http://localhost:3000")
PAYOUT_ADDRESS = os.environ.get(
    "PAYOUT_ADDRESS",
    "AbC2BEBTyK45VHyeFodk7HBmeTzJBUoBxAvbt8nTXEUy",
)


def request(base_url, path, method="GET", body=None, headers=None):
    payload = None if body is None else json.dumps(body).encode()
    request_headers = {} if headers is None else dict(headers)
    if payload is not None:
        request_headers.setdefault("Content-Type", "application/json")

    request = urllib.request.Request(
        f"{base_url}{path}",
        data=payload,
        headers=request_headers,
        method=method,
    )

    with urllib.request.urlopen(request, timeout=20) as response:
        return response.status, response.headers, response.read()


def main():
    email = f"frontend-{int(time.time())}@example.com"

    _, _, body = request(APP_BASE, "/")
    app_shell = body.decode()

    _, _, body = request(
        API_BASE,
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
        API_BASE,
        "/api/v1/auth/sign-in",
        method="POST",
        body={
            "email": email,
            "password": "Password123!",
        },
    )
    sign_in = json.loads(body)

    _, _, body = request(API_BASE, "/api/v1/auth/me", headers=auth_headers)
    me = json.loads(body)

    _, _, body = request(
        API_BASE,
        "/api/v1/me/invoices",
        method="POST",
        body={
            "amount_usdc": "1.234",
            "description": "Design work for homepage",
            "client_email": "client@example.com",
            "payout_address": PAYOUT_ADDRESS,
        },
        headers=auth_headers,
    )
    invoice = json.loads(body)

    _, _, body = request(API_BASE, "/api/v1/me/invoices", headers=auth_headers)
    invoices = json.loads(body)

    _, _, body = request(API_BASE, f"/api/v1/public/invoices/{invoice['id']}")
    public_invoice = json.loads(body)

    _, _, body = request(APP_BASE, f"/pay/{invoice['id']}")
    public_page = body.decode()

    qr_status, qr_headers, body = request(API_BASE, f"/api/v1/public/invoices/{invoice['id']}/qr.svg")
    qr_svg = body.decode()
    payment_recipient = invoice["payment_uri"].split("?", 1)[0].replace("solana:", "")
    public_payment_recipient = public_invoice.get("payment_uri", "").split("?", 1)[0].replace("solana:", "")

    summary = {
        "email": email,
        "root_page_has_copy": "Get paid in USDC. Instantly." in app_shell
        and "Create an invoice. Send a link. Funds settle directly to your wallet." in app_shell,
        "root_page_has_steps": "Create Invoice" in app_shell and "Send a Link" in app_shell and "Get Paid Instantly" in app_shell,
        "root_page_has_demo_cta": "Try Demo" in app_shell,
        "root_page_has_final_cta": "Ready to get paid without the friction?" in app_shell and "Create your first invoice" in app_shell,
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
        "invoice_usdc_ata": invoice["usdc_ata"],
        "payment_uri": invoice["payment_uri"],
        "payment_recipient": payment_recipient,
        "payment_uri_has_reference": "&reference=" in invoice["payment_uri"],
        "payment_uri_has_exact_reference": f"&reference={invoice['reference_pubkey']}" in invoice["payment_uri"],
        "private_invoice_uses_requested_payout": invoice["usdc_ata"] == PAYOUT_ADDRESS,
        "private_payment_uri_uses_wallet_pubkey": payment_recipient == invoice["wallet_pubkey"],
        "invoice_list_count": len(invoices),
        "private_invoice_has_client_email": any(
            row["id"] == invoice["id"] and row.get("client_email") == "client@example.com" for row in invoices
        ),
        "public_invoice_description": public_invoice.get("description"),
        "public_invoice_has_client_email": "client_email" in public_invoice,
        "public_invoice_reference_pubkey": public_invoice.get("reference_pubkey"),
        "public_invoice_payment_uri": public_invoice.get("payment_uri"),
        "public_payment_recipient": public_payment_recipient,
        "public_payment_uri_has_exact_reference": f"&reference={invoice['reference_pubkey']}" in public_invoice.get("payment_uri", ""),
        "public_payment_uri_matches_private": public_invoice.get("payment_uri") == invoice["payment_uri"],
        "public_payment_uri_uses_wallet_pubkey": public_payment_recipient == invoice["wallet_pubkey"],
        "public_page_has_heading": "Payments usually confirm in ~10-15 seconds." in public_page,
        "public_page_has_wallet_hint": "Use the Aurefly payment link or QR so your payment is credited automatically." in public_page,
        "qr_status": qr_status,
        "qr_content_type": qr_headers.get("Content-Type"),
        "qr_has_svg": "<svg" in qr_svg[:200],
    }

    if not summary["root_page_has_copy"]:
        raise SystemExit(f"landing page copy regression: {summary}")
    if not summary["root_page_has_steps"] or not summary["root_page_has_demo_cta"] or not summary["root_page_has_final_cta"]:
        raise SystemExit(f"landing page product framing regression: {summary}")
    if not summary["payment_uri_has_exact_reference"]:
        raise SystemExit(f"private payment URI missing exact reference: {summary}")
    if not summary["private_payment_uri_uses_wallet_pubkey"]:
        raise SystemExit(f"private payment URI must use wallet pubkey recipient: {summary}")
    if summary["public_invoice_reference_pubkey"] != invoice["reference_pubkey"]:
        raise SystemExit(f"public invoice reference mismatch: {summary}")
    if not summary["public_payment_uri_has_exact_reference"]:
        raise SystemExit(f"public payment URI missing exact reference: {summary}")
    if not summary["public_payment_uri_matches_private"]:
        raise SystemExit(f"public/private payment URI mismatch: {summary}")
    if not summary["public_payment_uri_uses_wallet_pubkey"]:
        raise SystemExit(f"public payment URI must use wallet pubkey recipient: {summary}")
    if not summary["public_page_has_heading"] or not summary["public_page_has_wallet_hint"]:
        raise SystemExit(f"public pay page regression: {summary}")
    if qr_status != 200 or "image/svg+xml" not in (summary["qr_content_type"] or "") or not summary["qr_has_svg"]:
        raise SystemExit(f"QR generation regression: {summary}")

    json.dump(summary, sys.stdout, separators=(",", ":"))


if __name__ == "__main__":
    main()
