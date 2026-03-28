import json
import subprocess
import sys
import time
import urllib.error
import urllib.request
import uuid
from hashlib import sha256


API_BASE = sys.argv[1] if len(sys.argv) > 1 else "http://localhost:8080/api/v1"
DB_SERVICE = sys.argv[2] if len(sys.argv) > 2 else "db"
PASSWORD = "CancelPassword123!"
USDC_ATA = "AbC2BEBTyK45VHyeFodk7HBmeTzJBUoBxAvbt8nTXEUy"
WALLET_PUBKEY = "3TLjMkmmBCnF7uWJ6DQY3Uc2ARw14PrRQ3vWQmgJ4hUM"
USDC_MINT = "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v"
BASE58_ALPHABET = "123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz"


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


def base58_encode(raw: bytes) -> str:
    value = int.from_bytes(raw, "big")
    encoded = ""
    while value:
        value, remainder = divmod(value, 58)
        encoded = BASE58_ALPHABET[remainder] + encoded

    leading_zeroes = len(raw) - len(raw.lstrip(b"\x00"))
    return ("1" * leading_zeroes) + (encoded or "1")


def invoice_reference_pubkey(invoice_id: uuid.UUID) -> str:
    digest = sha256(invoice_id.bytes).digest()
    return base58_encode(digest)


def insert_pending_invoice(user_id: str, invoice_id: str, reference_pubkey: str):
    sql = f"""
INSERT INTO invoices (
    id,
    user_id,
    reference_pubkey,
    subtotal_usdc,
    platform_fee_usdc,
    platform_fee_bps,
    amount_usdc,
    description,
    client_email,
    status,
    wallet_address,
    wallet_pubkey,
    usdc_ata,
    usdc_mint
)
VALUES (
    '{invoice_id}',
    '{user_id}',
    '{reference_pubkey}',
    0.25,
    0,
    0,
    0.25,
    'Cancel invoice smoke (inserted)',
    NULL,
    'pending',
    '{USDC_ATA}',
    '{WALLET_PUBKEY}',
    '{USDC_ATA}',
    '{USDC_MINT}'
);
""".strip()

    process = subprocess.run(
        [
            "cmd.exe",
            "/c",
            "docker",
            "compose",
            "exec",
            "-T",
            DB_SERVICE,
            "psql",
            "-U",
            "solana_pay",
            "-d",
            "solana_pay",
        ],
        input=sql,
        text=True,
        capture_output=True,
        timeout=30,
        check=False,
    )

    if process.returncode != 0:
        raise RuntimeError(
            f"failed to insert pending invoice via psql:\nstdout={process.stdout}\nstderr={process.stderr}"
        )


def main():
    email = f"cancel-inserted-{int(time.time())}@example.com"
    sign_up_status, sign_up = request_json(
        "POST",
        f"{API_BASE}/auth/sign-up",
        {"email": email, "password": PASSWORD},
    )
    token = sign_up["token"]
    user_id = sign_up["user"]["id"]

    invoice_id = str(uuid.uuid4())
    reference_pubkey = invoice_reference_pubkey(uuid.UUID(invoice_id))
    insert_pending_invoice(user_id, invoice_id, reference_pubkey)

    cancel_status, cancelled = request_json(
        "POST",
        f"{API_BASE}/me/invoices/{invoice_id}/cancel",
        token=token,
    )

    if cancel_status != 200 or not isinstance(cancelled, dict):
        raise SystemExit(
            json.dumps(
                {
                    "email": email,
                    "sign_up_status": sign_up_status,
                    "invoice_id": invoice_id,
                    "reference_pubkey": reference_pubkey,
                    "cancel_status": cancel_status,
                    "cancel_response": cancelled,
                },
                indent=2,
            )
        )

    list_status, invoices = request_json(
        "GET",
        f"{API_BASE}/me/invoices",
        token=token,
    )

    public_status, public_invoice = request_json(
        "GET",
        f"{API_BASE}/public/invoices/{invoice_id}",
    )

    second_cancel_status, second_cancel = request_json(
        "POST",
        f"{API_BASE}/me/invoices/{invoice_id}/cancel",
        token=token,
    )

    summary = {
        "email": email,
        "sign_up_status": sign_up_status,
        "cancel_status": cancel_status,
        "list_status": list_status,
        "public_status": public_status,
        "second_cancel_status": second_cancel_status,
        "invoice_id": invoice_id,
        "reference_pubkey": reference_pubkey,
        "cancelled_status": cancelled["status"],
        "listed_status": next((row["status"] for row in invoices if row["id"] == invoice_id), None),
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
