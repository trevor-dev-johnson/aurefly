ALTER TABLE invoices
    ADD COLUMN subtotal_usdc NUMERIC(20, 6),
    ADD COLUMN platform_fee_usdc NUMERIC(20, 6) NOT NULL DEFAULT 0::numeric,
    ADD COLUMN platform_fee_bps SMALLINT NOT NULL DEFAULT 0;

UPDATE invoices
SET
    subtotal_usdc = amount_usdc,
    platform_fee_usdc = 0::numeric,
    platform_fee_bps = 0
WHERE subtotal_usdc IS NULL;

ALTER TABLE invoices
    ALTER COLUMN subtotal_usdc SET NOT NULL;

ALTER TABLE invoices
    ADD CONSTRAINT invoices_subtotal_usdc_positive CHECK (subtotal_usdc > 0),
    ADD CONSTRAINT invoices_platform_fee_usdc_nonnegative CHECK (platform_fee_usdc >= 0),
    ADD CONSTRAINT invoices_platform_fee_bps_nonnegative CHECK (platform_fee_bps >= 0);
