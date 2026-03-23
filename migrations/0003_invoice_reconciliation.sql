ALTER TABLE invoices
    ADD COLUMN paid_at TIMESTAMPTZ;

CREATE INDEX idx_invoices_pending_reconciliation
    ON invoices (status, usdc_ata, usdc_mint, amount_usdc, created_at);
