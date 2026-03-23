import json
import sys
import urllib.request
import uuid


BASE_URL = sys.argv[1] if len(sys.argv) > 1 else "http://localhost:8080/api/v1"
AMOUNT_USDC = sys.argv[2] if len(sys.argv) > 2 else "49.99"


def request(method: str, path: str, payload: dict | None = None) -> dict:
    body = None
    headers = {}

    if payload is not None:
        body = json.dumps(payload).encode("utf-8")
        headers["Content-Type"] = "application/json"

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
    user = request(
        "POST",
        "/users",
        {
            "email": email,
            "name": "Test Merchant",
        },
    )
    invoice = request(
        "POST",
        "/invoices",
        {
            "user_id": user["id"],
            "amount_usdc": AMOUNT_USDC,
        },
    )
    fetched_invoice = request("GET", f"/invoices/{invoice['id']}")

    print(
        json.dumps(
            {
                "user": user,
                "created_invoice": invoice,
                "fetched_invoice": fetched_invoice,
            },
            indent=2,
        )
    )


if __name__ == "__main__":
    main()
