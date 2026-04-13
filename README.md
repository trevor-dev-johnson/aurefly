# Aurefly

Aurefly is a non-custodial USDC invoicing platform on Solana.

Create an invoice. Share a link. Get paid instantly.

No custody. No chargebacks. No bullshit.

## What It Does

- Create USDC invoices in seconds
- Share a simple payment link or QR
- Get paid directly to your wallet
- Track invoice status in real time

Aurefly never holds funds. Payments go straight from the payer to you.

## How It Works

1. Create an invoice
2. Send the payment link
3. Customer pays with USDC
4. Payment is detected and confirmed on-chain

That's it.

## Why Aurefly

- Non-custodial: your keys, your money
- Fast settlement: Solana finality
- No intermediaries: no Stripe-style lockups
- Built for freelancers and online work

If you can send a link, you can get paid.

## Live App

- Frontend: [https://aurefly.com](https://aurefly.com)
- API: [https://aurefly-production.up.railway.app](https://aurefly-production.up.railway.app)

## Supported Payments

- USDC on Solana mainnet

Mainnet USDC mint:

- `EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v`

## Security

- Payments are verified on-chain
- No funds are ever held by Aurefly
- Invoice matching is reference-based
- Expired invoices are automatically invalidated
- Confirmed payments are detector-only; there is no public payment-ingestion route

## Who This Is For

- Freelancers
- Developers
- Online businesses
- Anyone tired of waiting days to get paid

## Current Status

Aurefly is live and actively being improved.

## Repo Structure

This repo runs as two separate apps:

- `aurefly-web/`: Next.js frontend
- `src/`: Rust `axum` API and payment detector

The Rust service is API-only. It does not serve the website.

## Stack

- Next.js App Router frontend
- Supabase Auth for email/password sessions
- Rust + Axum backend API
- SQLx + Postgres
- Solana mainnet
- Helius primary RPC with optional QuickNode fallback

## Local Development

1. Start Postgres and the Rust API:

```bash
docker compose up --build
```

2. Confirm backend health:

```bash
curl http://localhost:8080/api/v1/health
```

3. Start the Next frontend:

```bash
cd aurefly-web
npm install
npm run dev
```

4. Open the app:

```text
http://localhost:3000
```

## Frontend Env

Create `aurefly-web/.env.local`:

```bash
NEXT_PUBLIC_API_URL=http://localhost:8080
NEXT_PUBLIC_SUPABASE_URL=
NEXT_PUBLIC_SUPABASE_PUBLISHABLE_DEFAULT_KEY=
```

There is also an example file at `aurefly-web/.env.example`.

## Deployment Shape

Recommended production split:

- Frontend: Vercel
- Backend API: Railway

Example domains:

- `https://aurefly.com` -> Vercel frontend
- `https://api.aurefly.com` or Railway domain -> Rust API

The frontend must point `NEXT_PUBLIC_API_URL` at the backend domain, not the frontend domain.

## Railway Backend

Railway builds the Rust API from the root `Dockerfile` and healthchecks:

```text
/api/v1/health
```

Minimum backend env:

```bash
DATABASE_URL=<Railway Postgres connection string>
PORT=8080
ALLOWED_ORIGINS=https://aurefly.com,https://www.aurefly.com
ADMIN_EMAILS=ops@example.com
SUPABASE_URL=https://<project-ref>.supabase.co
SUPABASE_PUBLISHABLE_KEY=<your-supabase-publishable-key>
HELIUS_API_KEY=<your-helius-key>
SOLANA_FALLBACK_RPC_URL=<optional QuickNode HTTPS URL>
SOLANA_FALLBACK_WS_URL=<optional QuickNode WSS URL>
TREASURY_WALLET_JSON=<optional>
SOLANA_FEE_PAYER_JSON=<optional>
INVOICE_PENDING_TTL_SECS=1800
PAYMENT_DETECTOR_POLL_INTERVAL_SECS=5
PAYMENT_DETECTOR_FAST_POLL_INTERVAL_SECS=6
PAYMENT_DETECTOR_MEDIUM_POLL_INTERVAL_SECS=20
PAYMENT_DETECTOR_SLOW_POLL_INTERVAL_SECS=60
PAYMENT_DETECTOR_FAST_WINDOW_SECS=120
PAYMENT_DETECTOR_MEDIUM_WINDOW_SECS=900
PAYMENT_DETECTOR_MAX_TARGETS_PER_CYCLE=6
PAYMENT_DETECTOR_MAX_ACTIVE_LOGS_SUBSCRIPTIONS=12
PAYMENT_DETECTOR_MAX_IDLE_BACKOFF_SECS=300
PAYMENT_DETECTOR_SIGNATURE_DEDUPE_TTL_SECS=300
RUST_LOG=info
```

Detector behavior now prioritizes fresh pending invoices, limits active websocket subscriptions, and backs older unpaid targets off aggressively instead of polling every open destination at a flat 10-second cadence.

## API Overview

Public endpoints:

- `GET /api/v1/health`
- `GET /api/v1/auth/me`
- `GET /api/v1/public/invoices/{invoice_id}`
- `GET /api/v1/public/invoices/{invoice_id}/qr.svg`

Authentication is handled in the Next.js app with Supabase Auth. The Rust API only trusts bearer tokens issued by Supabase for protected routes.

Authenticated invoice management:

- `GET /api/v1/me/invoices`
- `POST /api/v1/me/invoices`
- `POST /api/v1/me/invoices/{invoice_id}/cancel`

Internal admin reconciliation:

- `GET /api/v1/admin/unmatched-payments`
- `GET /api/v1/admin/unmatched-payments/{id}`
- `POST /api/v1/admin/unmatched-payments/{id}/link`
- `POST /api/v1/admin/unmatched-payments/{id}/status`
- `POST /api/v1/admin/unmatched-payments/{id}/retry`

Only emails listed in `ADMIN_EMAILS` can access the reconciliation layer.

## Settlement Rules

- Merchants can paste a wallet address or USDC token account
- Aurefly derives and stores the real USDC ATA internally
- Solana Pay links use the merchant wallet pubkey as recipient
- Invoices stay tied to the merchant's own wallet and USDC settlement account
- Pending invoices automatically expire after the configured TTL
- Invoices are credited only when all required conditions match:
  - destination USDC ATA
  - USDC mint
  - exact reference
  - pending invoice state

Manual transfers without the Aurefly link or QR are stored as unmatched and are not auto-credited.
