# Aurefly Web

This is the Next.js frontend for Aurefly.

It talks to the existing Rust API through `NEXT_PUBLIC_API_URL`.

## Local Development

1. Make sure the Rust API is running on `http://localhost:8080`
2. Copy `.env.example` to `.env.local`
3. Start the app:

```bash
npm install
npm run dev
```

Open [http://localhost:3000](http://localhost:3000).

## Env

```bash
NEXT_PUBLIC_API_URL=http://localhost:8080
```

## Routes

- `/`
- `/auth`
- `/dashboard`
- `/pay/[invoiceId]`

## Production

Recommended setup:

- Vercel hosts this app
- Railway hosts the Rust API
- `NEXT_PUBLIC_API_URL` points at the backend API domain
