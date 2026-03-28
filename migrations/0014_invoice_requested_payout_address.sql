ALTER TABLE invoices
ADD COLUMN requested_payout_address TEXT;

UPDATE invoices
SET requested_payout_address = COALESCE(NULLIF(TRIM(wallet_pubkey), ''), usdc_ata)
WHERE requested_payout_address IS NULL;

ALTER TABLE invoices
ALTER COLUMN requested_payout_address SET NOT NULL;

ALTER TABLE invoices
ADD CONSTRAINT invoices_requested_payout_address_not_blank
CHECK (NULLIF(TRIM(requested_payout_address), '') IS NOT NULL);
