CREATE TABLE unmatched_payments (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    signature TEXT NOT NULL UNIQUE,
    destination_wallet TEXT NOT NULL,
    amount_usdc NUMERIC(20, 6) NOT NULL CHECK (amount_usdc > 0),
    sender_wallet TEXT,
    reference_pubkey TEXT,
    seen_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    reason TEXT NOT NULL
);

CREATE INDEX idx_unmatched_payments_seen_at ON unmatched_payments (seen_at DESC);
CREATE INDEX idx_unmatched_payments_reason ON unmatched_payments (reason);
