# Aurefly

Aurefly is a USDC invoicing app on Solana. This repo ships a Rust `axum` backend, Postgres, SQL migrations, and a server-rendered static frontend for sign-in, dashboard, invoice creation, and public pay pages.

Invoices are created by authenticated users under `/api/v1/me/invoices`. Each invoice stores a real USDC settlement target, returns a Solana Pay URI with a per-invoice reference, and is marked paid only by the on-chain detector.

## Stack

- Rust + Axum for the HTTP API and static frontend hosting
- SQLx for Postgres access and migrations
- Postgres 16 via Docker Compose

## Project Structure

```text
.
|-- docker-compose.yml
|-- migrations/
|-- src/
|   |-- db.rs
|   |-- routes/
|   |-- services/
|   `-- models/
```

## Quick Start

1. Copy `.env.example` to `.env` if you want to run the API outside Docker.
2. Start the stack:

```bash
docker compose up --build
```

3. Check health:

```bash
curl http://localhost:8080/api/v1/health
```

4. On first boot the API creates `data/treasury-wallet.json` and reuses it on later restarts if you do not provide wallet secrets via env vars.

5. The API pre-creates the treasury wallet's USDC ATA at startup. In Docker Compose it uses your local Solana keypair at `%USERPROFILE%\.config\solana\id.json` as the ATA fee payer so the treasury wallet can stay unfunded.

6. Open the app at `http://localhost:8080`, create an account, and create invoices from the dashboard.

## Hosted Deployment

For hosted environments, do not rely on an ephemeral container filesystem for key material.

- `TREASURY_WALLET_JSON`: optional full Solana keypair JSON array for the treasury wallet. If set, it overrides `TREASURY_WALLET_PATH`.
- `SOLANA_FEE_PAYER_JSON`: optional full Solana keypair JSON array for the ATA fee payer. If set, it overrides `SOLANA_FEE_PAYER_PATH`.

That lets you keep both wallets in provider secrets instead of mounting local files.

## Railway

This repo now includes [railway.toml](./railway.toml) so Railway can build from the root `Dockerfile`, use `/api/v1/health` as the deployment health check, and restart on failure.

Minimum production variables:

```bash
DATABASE_URL=<Railway Postgres connection string>
PORT=8080
ALLOWED_ORIGINS=https://aurefly.com,https://www.aurefly.com
HELIUS_API_KEY=<your-helius-key>
TREASURY_WALLET_JSON=<contents of data/treasury-wallet.json>
SOLANA_FEE_PAYER_JSON=<contents of your funded Solana keypair json>
RUST_LOG=info
```

Recommended Railway flow:

1. Create a new Railway project.
2. Add a PostgreSQL service.
3. Deploy this repo as a service from the root `Dockerfile`.
4. Set the environment variables above on the app service.
5. Generate a public domain for the app service.
6. Verify `/api/v1/health` and then create a live invoice.

Security defaults in this repo:

- CORS is restricted to `ALLOWED_ORIGINS`.
- auth endpoints have a basic in-memory per-client rate limit.
- internal server errors are logged server-side but returned to clients as a generic `internal server error`.

## Dedicated RPC

The app can use a dedicated provider like Helius instead of the public Solana mainnet RPC.

- Fastest path: set `HELIUS_API_KEY` in `.env` and leave `SOLANA_RPC_URL` blank.
- If you already have a full provider URL, set `SOLANA_RPC_URL` directly instead.
- Docker Compose now passes both env vars through to the API, and the app resolves them in this order:
  1. `SOLANA_RPC_URL`
  2. derived Helius mainnet URL from `HELIUS_API_KEY`
  3. public fallback `https://api.mainnet-beta.solana.com`

Example:

```bash
HELIUS_API_KEY=your-helius-api-key
```

Or:

```bash
SOLANA_RPC_URL=https://mainnet.helius-rpc.com/?api-key=your-helius-api-key
```

## Treasury Notes

- Mainnet USDC mint is hardcoded to `EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v`.
- The invoice pay-to address is the USDC ATA, not the system wallet pubkey.
- Before any invoice is created, startup ensures the treasury USDC ATA exists on-chain.
- If the treasury wallet is new, set `SOLANA_FEE_PAYER_PATH` to a funded Solana keypair that can pay the one-time ATA rent.

## API Overview

The public API surface is intentionally small:

- `POST /api/v1/auth/sign-up`
- `POST /api/v1/auth/sign-in`
- `POST /api/v1/auth/logout`
- `GET /api/v1/auth/me`
- `GET /api/v1/health`
- `GET /api/v1/public/invoices/{invoice_id}`
- `GET /api/v1/public/invoices/{invoice_id}/qr.svg`

Authenticated invoice management lives under `/api/v1/me/invoices`.

### Sign up

```bash
curl -X POST http://localhost:8080/api/v1/auth/sign-up \
  -H "Content-Type: application/json" \
  -d '{"email":"merchant@example.com","password":"correct horse battery staple","name":"Merchant"}'
```

### Create an invoice

Use the returned bearer token from sign-up or sign-in.

```bash
curl -X POST http://localhost:8080/api/v1/me/invoices \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer <TOKEN>" \
  -d '{"amount_usdc":"49.99","description":"Design work","client_email":"client@example.com","payout_address":"<REAL_USDC_DESTINATION>"}'
```

Notes:

- `amount_usdc` is accepted as a string to preserve decimal precision.
- `payout_address` is required and must be the merchant's existing USDC associated token account (ATA).
- invoice responses include `wallet_pubkey`, `usdc_ata`, `usdc_mint`, and `payment_uri`.
- confirmed payments are recorded internally by the detector after on-chain verification. There is no public payment-ingestion endpoint.
