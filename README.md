# Aurefly

Aurefly is a non-custodial USDC invoicing app on Solana.

This repo now runs as two separate apps:

- [aurefly-web](C:/Users/Trevor/dev/solana-pay/aurefly-web): Next.js frontend
- [src](C:/Users/Trevor/dev/solana-pay/src): Rust `axum` API + payment detector

The Rust service is API-only. It no longer serves the website.

## Stack

- Next.js App Router frontend
- Rust + Axum backend API
- SQLx + Postgres
- Solana mainnet + Helius RPC

## Local Development

1. Start Postgres + Rust API:

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

4. Open the frontend:

```text
http://localhost:3000
```

## Frontend Env

Create [aurefly-web/.env.local](C:/Users/Trevor/dev/solana-pay/aurefly-web/.env.local):

```bash
NEXT_PUBLIC_API_URL=http://localhost:8080
```

There is also an example file at [aurefly-web/.env.example](C:/Users/Trevor/dev/solana-pay/aurefly-web/.env.example).

## Deployment Shape

Recommended production split:

- Frontend: Vercel
- Backend API: Railway

Example domains:

- `https://aurefly.com` -> Vercel frontend
- `https://api.aurefly.com` or Railway domain -> Rust API

The frontend must point `NEXT_PUBLIC_API_URL` at the backend domain, not the frontend domain.

## Railway Backend

Railway builds the Rust API from the root [Dockerfile](C:/Users/Trevor/dev/solana-pay/Dockerfile) and healthchecks:

```text
/api/v1/health
```

Minimum backend env:

```bash
DATABASE_URL=<Railway Postgres connection string>
PORT=8080
ALLOWED_ORIGINS=https://aurefly.com,https://www.aurefly.com
HELIUS_API_KEY=<your-helius-key>
TREASURY_WALLET_JSON=<optional>
SOLANA_FEE_PAYER_JSON=<optional>
INVOICE_PENDING_TTL_SECS=1800
RUST_LOG=info
```

## API Overview

Public endpoints:

- `GET /api/v1/health`
- `POST /api/v1/auth/sign-up`
- `POST /api/v1/auth/sign-in`
- `POST /api/v1/auth/logout`
- `GET /api/v1/auth/me`
- `GET /api/v1/public/invoices/{invoice_id}`
- `GET /api/v1/public/invoices/{invoice_id}/qr.svg`

Authenticated invoice management:

- `GET /api/v1/me/invoices`
- `POST /api/v1/me/invoices`
- `POST /api/v1/me/invoices/{invoice_id}/cancel`

## Settlement Rules

- Mainnet USDC mint is fixed to `EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v`
- Merchants can paste a wallet address or USDC token account
- Aurefly derives and stores the real USDC ATA internally
- Solana Pay links use the merchant wallet pubkey as recipient
- Pending invoices automatically expire after the configured TTL so the detector does not keep scanning stale requests forever
- The detector credits invoices only by:
  - destination USDC ATA
  - USDC mint
  - exact reference
  - pending invoice state

Manual transfers without the Aurefly link/QR are stored as unmatched and are not auto-credited.

## Security Notes

- The backend is API-only; no website is served from Railway anymore
- CORS is restricted by `ALLOWED_ORIGINS`
- Confirmed payments are detector-only; there is no public payment-ingestion route
- Invoice cancellation is allowed only while an invoice is still `pending`
