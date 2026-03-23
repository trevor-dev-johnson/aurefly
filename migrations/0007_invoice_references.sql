ALTER TABLE invoices
    ADD COLUMN reference_pubkey TEXT;

CREATE UNIQUE INDEX idx_invoices_reference_pubkey_unique
    ON invoices (reference_pubkey)
    WHERE reference_pubkey IS NOT NULL;
