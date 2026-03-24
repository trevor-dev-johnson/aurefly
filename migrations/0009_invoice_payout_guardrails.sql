ALTER TABLE invoices
    ALTER COLUMN wallet_address SET NOT NULL,
    ALTER COLUMN wallet_pubkey SET NOT NULL,
    ALTER COLUMN usdc_ata SET NOT NULL,
    ALTER COLUMN usdc_mint SET NOT NULL;

ALTER TABLE invoices
    ADD CONSTRAINT invoices_wallet_address_not_blank CHECK (length(btrim(wallet_address)) > 0),
    ADD CONSTRAINT invoices_wallet_pubkey_not_blank CHECK (length(btrim(wallet_pubkey)) > 0),
    ADD CONSTRAINT invoices_usdc_ata_not_blank CHECK (length(btrim(usdc_ata)) > 0),
    ADD CONSTRAINT invoices_usdc_mint_not_blank CHECK (length(btrim(usdc_mint)) > 0);
