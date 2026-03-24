# Aurefly

Aurefly is a Stripe-like Solana payments backend. This repo ships a Rust API, Postgres, SQL migrations, and a basic service layout for users, wallet addresses, invoices, and payments.

Invoices now resolve to a real USDC destination flow:

- the API generates or loads one local treasury wallet
- it derives that wallet's USDC associated token account (ATA)
- every invoice stores and returns the treasury wallet pubkey, the USDC ATA, and the hardcoded mainnet USDC mint

## Stack

- Rust + Axum for the HTTP API
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

4. On first boot the API creates `data/treasury-wallet.json` and reuses it on later restarts.

5. The API pre-creates the treasury wallet's USDC ATA at startup. In Docker Compose it uses your local Solana keypair at `%USERPROFILE%\.config\solana\id.json` as the ATA fee payer so the treasury wallet can stay unfunded.

6. Invoice responses return the treasury wallet pubkey and the USDC ATA to pay.

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

## Phase 1 API

### Create a user

```bash
curl -X POST http://localhost:8080/api/v1/users \
  -H "Content-Type: application/json" \
  -d '{"email":"merchant@example.com","name":"Merchant"}'
```

### Create a wallet address

```bash
curl -X POST http://localhost:8080/api/v1/wallet-addresses \
  -H "Content-Type: application/json" \
  -d '{"user_id":"<USER_ID>","wallet_pubkey":"<REAL_SOLANA_WALLET_PUBKEY>","label":"Primary treasury"}'
```

The API derives the wallet's USDC ATA and stores both the owner pubkey and the USDC token account.

### Create an invoice

`amount_usdc` is accepted as a string to preserve decimal precision. The API does not accept a destination address here anymore; it uses the configured treasury wallet and returns its derived USDC ATA.

```bash
curl -X POST http://localhost:8080/api/v1/invoices \
  -H "Content-Type: application/json" \
  -d '{"user_id":"<USER_ID>","amount_usdc":"49.99"}'
```

Invoice responses now include:

- `wallet_pubkey`: the treasury wallet owner
- `usdc_ata`: the USDC token account customers should pay
- `usdc_mint`: the hardcoded mainnet USDC mint

### Payment detection

Confirmed payments are now recorded internally by the detector after on-chain verification. There is no public payment-ingestion endpoint.
