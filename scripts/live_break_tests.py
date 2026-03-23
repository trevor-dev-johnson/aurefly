import json
import subprocess
import sys
import time
import http.client
import urllib.request
import uuid
from decimal import Decimal
from pathlib import Path


BASE_URL = sys.argv[1] if len(sys.argv) > 1 else "http://localhost:8080/api/v1"
SPL_TOKEN_EXE = (
    Path(sys.argv[2])
    if len(sys.argv) > 2
    else Path.home() / ".cargo" / "bin" / "spl-token.exe"
)
USDC_MINT = "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v"
DOCKER_COMPOSE = ["cmd.exe", "/c", "docker compose"]
SCENARIO_TIMEOUT_SECS = 180
POLL_INTERVAL_SECS = 2
HTTP_RETRY_ATTEMPTS = 15
HTTP_RETRY_DELAY_SECS = 2


def request(method: str, path: str, payload: dict | None = None):
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

    last_error = None
    for attempt in range(1, HTTP_RETRY_ATTEMPTS + 1):
        try:
            with urllib.request.urlopen(req, timeout=30) as response:
                return json.loads(response.read().decode("utf-8"))
        except Exception as exc:
            last_error = exc
            if attempt == HTTP_RETRY_ATTEMPTS:
                raise

            time.sleep(HTTP_RETRY_DELAY_SECS)

    raise RuntimeError(f"request failed after retries: {last_error}")


def run(command: list[str], cwd: str | None = None) -> str:
    result = subprocess.run(
        command,
        cwd=cwd,
        check=True,
        capture_output=True,
        text=True,
    )
    return result.stdout.strip()


def create_user() -> dict:
    email = f"merchant+{uuid.uuid4().hex[:8]}@example.com"
    return request(
        "POST",
        "/users",
        {
            "email": email,
            "name": "Break Test Merchant",
        },
    )


def create_invoice(amount_usdc: str) -> dict:
    user = create_user()
    return request(
        "POST",
        "/invoices",
        {
            "user_id": user["id"],
            "amount_usdc": amount_usdc,
        },
    )


def get_invoice(invoice_id: str) -> dict:
    return request("GET", f"/invoices/{invoice_id}")


def list_payments() -> list[dict]:
    return request("GET", "/payments")


def transfer_usdc(amount_usdc: str, recipient_token_account: str) -> str:
    return run(
        [
            str(SPL_TOKEN_EXE),
            "-u",
            "mainnet-beta",
            "transfer",
            USDC_MINT,
            amount_usdc,
            recipient_token_account,
            "--allow-non-system-account-recipient",
            "--output",
            "json",
        ]
    )


def wait_for_invoices_paid(invoice_ids: list[str]) -> dict[str, dict]:
    deadline = time.time() + SCENARIO_TIMEOUT_SECS
    pending = set(invoice_ids)
    latest: dict[str, dict] = {}

    while pending and time.time() < deadline:
        for invoice_id in list(pending):
            invoice = get_invoice(invoice_id)
            latest[invoice_id] = invoice
            if invoice["status"] == "paid":
                pending.remove(invoice_id)

        if pending:
            time.sleep(POLL_INTERVAL_SECS)

    if pending:
        raise RuntimeError(f"invoices did not become paid before timeout: {sorted(pending)}")

    return latest


def payments_for_invoices(invoice_ids: list[str]) -> list[dict]:
    invoice_id_set = set(invoice_ids)
    return [payment for payment in list_payments() if payment["invoice_id"] in invoice_id_set]


def require(condition: bool, message: str) -> None:
    if not condition:
        raise RuntimeError(message)


def wait_for_health() -> None:
    deadline = time.time() + SCENARIO_TIMEOUT_SECS

    while time.time() < deadline:
        try:
            health = request("GET", "/health")
            if health.get("status") == "ok":
                return
        except Exception:
            pass

        time.sleep(POLL_INTERVAL_SECS)

    raise RuntimeError("api did not become healthy before timeout")


def rapid_fire_same_amount() -> dict:
    invoices = [create_invoice("0.031") for _ in range(3)]
    invoice_ids = [invoice["id"] for invoice in invoices]

    for _ in invoices:
        transfer_usdc("0.031", invoices[0]["usdc_ata"])

    invoice_states = wait_for_invoices_paid(invoice_ids)
    payments = payments_for_invoices(invoice_ids)

    require(len(payments) == 3, "rapid-fire test expected 3 payment rows")
    require(
        len({payment["tx_signature"] for payment in payments}) == 3,
        "rapid-fire test found duplicate tx signatures",
    )
    require(
        all(invoice_states[invoice_id]["status"] == "paid" for invoice_id in invoice_ids),
        "rapid-fire test left an invoice unpaid",
    )

    return {
        "invoices": invoice_states,
        "payments": payments,
    }


def mixed_amounts_out_of_order() -> dict:
    invoices = {
        "0.052": create_invoice("0.052"),
        "0.104": create_invoice("0.104"),
        "0.156": create_invoice("0.156"),
    }

    for amount in ["0.156", "0.104", "0.052"]:
        transfer_usdc(amount, invoices[amount]["usdc_ata"])

    invoice_ids = [invoice["id"] for invoice in invoices.values()]
    invoice_states = wait_for_invoices_paid(invoice_ids)
    payments = payments_for_invoices(invoice_ids)

    require(len(payments) == 3, "mixed-amount test expected 3 payment rows")

    for amount, invoice in invoices.items():
        matches = [payment for payment in payments if payment["invoice_id"] == invoice["id"]]
        require(len(matches) == 1, f"mixed-amount test expected 1 payment for invoice {invoice['id']}")
        require(
            Decimal(matches[0]["amount_usdc"]) == Decimal(amount),
            f"mixed-amount test recorded the wrong amount for invoice {invoice['id']}",
        )

    return {
        "invoices": invoice_states,
        "payments": payments,
    }


def overpayment() -> dict:
    invoice = create_invoice("0.041")
    transfer_usdc("0.073", invoice["usdc_ata"])

    invoice_state = wait_for_invoices_paid([invoice["id"]])[invoice["id"]]
    payments = payments_for_invoices([invoice["id"]])

    require(len(payments) == 1, "overpayment test expected 1 payment row")
    require(
        Decimal(payments[0]["amount_usdc"]) == Decimal("0.073"),
        "overpayment test did not record the sent amount",
    )

    return {
        "invoice": invoice_state,
        "payment": payments[0],
    }


def restart_recovery(repo_root: str) -> dict:
    invoice = create_invoice("0.082")

    run(DOCKER_COMPOSE + ["stop", "api"], cwd=repo_root)
    transfer_usdc("0.082", invoice["usdc_ata"])
    run(DOCKER_COMPOSE + ["start", "api"], cwd=repo_root)
    wait_for_health()

    invoice_state = wait_for_invoices_paid([invoice["id"]])[invoice["id"]]
    payments_before_restart = payments_for_invoices([invoice["id"]])
    require(len(payments_before_restart) == 1, "restart-recovery test expected 1 payment row after recovery")

    run(DOCKER_COMPOSE + ["restart", "api"], cwd=repo_root)
    wait_for_health()
    time.sleep(12)

    payments_after_restart = payments_for_invoices([invoice["id"]])
    require(
        len(payments_after_restart) == 1,
        "restart-recovery test created duplicate payment rows after restart",
    )

    return {
        "invoice": invoice_state,
        "payments": payments_after_restart,
    }


def run_scenario(name: str, scenario) -> dict:
    started_at = time.time()

    try:
        return {
            "status": "passed",
            "duration_secs": round(time.time() - started_at, 2),
            "result": scenario(),
        }
    except Exception as exc:
        return {
            "status": "failed",
            "duration_secs": round(time.time() - started_at, 2),
            "error": str(exc),
        }


def main() -> None:
    repo_root = str(Path(__file__).resolve().parents[1])

    summary = {
        "rapid_fire_same_amount": run_scenario(
            "rapid_fire_same_amount", rapid_fire_same_amount
        ),
        "mixed_amounts_out_of_order": run_scenario(
            "mixed_amounts_out_of_order", mixed_amounts_out_of_order
        ),
        "overpayment": run_scenario("overpayment", overpayment),
        "restart_recovery": run_scenario(
            "restart_recovery", lambda: restart_recovery(repo_root)
        ),
    }

    print(json.dumps(summary, indent=2))

    if any(result["status"] != "passed" for result in summary.values()):
        raise SystemExit(1)


if __name__ == "__main__":
    main()
