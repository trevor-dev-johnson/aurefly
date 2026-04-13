ALTER TABLE unmatched_payments
DROP CONSTRAINT IF EXISTS unmatched_payments_status_check;

ALTER TABLE unmatched_payments
ADD COLUMN notes TEXT,
ADD COLUMN metadata JSONB NOT NULL DEFAULT '{}'::jsonb;

ALTER TABLE unmatched_payments
ADD CONSTRAINT unmatched_payments_status_check
CHECK (
    status IN (
        'pending',
        'reviewed',
        'resolved',
        'ignored',
        'refunded_manually',
        'needs_investigation'
    )
);

CREATE TABLE unmatched_payment_audit_events (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    unmatched_payment_id UUID NOT NULL REFERENCES unmatched_payments(id) ON DELETE CASCADE,
    actor_user_id UUID REFERENCES users(id) ON DELETE SET NULL,
    actor_email TEXT NOT NULL,
    action TEXT NOT NULL,
    previous_status TEXT,
    next_status TEXT,
    linked_invoice_id UUID REFERENCES invoices(id) ON DELETE SET NULL,
    note TEXT,
    metadata JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_unmatched_payment_audit_events_payment_id
    ON unmatched_payment_audit_events (unmatched_payment_id, created_at DESC);
CREATE INDEX idx_unmatched_payment_audit_events_action
    ON unmatched_payment_audit_events (action);
