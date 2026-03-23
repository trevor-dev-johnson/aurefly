ALTER TABLE payments
    ADD COLUMN finalized_at TIMESTAMPTZ,
    ADD COLUMN slot BIGINT;

CREATE INDEX idx_payments_finalized_at ON payments (finalized_at);
