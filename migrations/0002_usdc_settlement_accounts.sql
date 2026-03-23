ALTER TABLE wallet_addresses
    ADD COLUMN wallet_pubkey TEXT,
    ADD COLUMN usdc_ata TEXT,
    ADD COLUMN usdc_mint TEXT NOT NULL DEFAULT 'EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v';

UPDATE wallet_addresses
SET wallet_pubkey = address,
    usdc_ata = address
WHERE wallet_pubkey IS NULL;

ALTER TABLE wallet_addresses
    ALTER COLUMN wallet_pubkey SET NOT NULL,
    ALTER COLUMN usdc_ata SET NOT NULL;

ALTER TABLE invoices
    ADD COLUMN wallet_pubkey TEXT,
    ADD COLUMN usdc_ata TEXT,
    ADD COLUMN usdc_mint TEXT NOT NULL DEFAULT 'EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v';

UPDATE invoices
SET wallet_pubkey = wallet_address,
    usdc_ata = wallet_address
WHERE wallet_pubkey IS NULL;

ALTER TABLE invoices
    ALTER COLUMN wallet_pubkey SET NOT NULL,
    ALTER COLUMN usdc_ata SET NOT NULL;

ALTER TABLE payments
    ADD COLUMN recipient_token_account TEXT,
    ADD COLUMN token_mint TEXT NOT NULL DEFAULT 'EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v';

UPDATE payments AS payments
SET recipient_token_account = invoices.usdc_ata
FROM invoices
WHERE invoices.id = payments.invoice_id
  AND payments.recipient_token_account IS NULL;

ALTER TABLE payments
    ALTER COLUMN recipient_token_account SET NOT NULL;

CREATE INDEX idx_wallet_addresses_wallet_pubkey ON wallet_addresses (wallet_pubkey);
CREATE INDEX idx_wallet_addresses_usdc_ata ON wallet_addresses (usdc_ata);
CREATE INDEX idx_invoices_usdc_ata ON invoices (usdc_ata);
CREATE INDEX idx_payments_recipient_token_account ON payments (recipient_token_account);
