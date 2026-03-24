ALTER TABLE unmatched_payments
ADD COLUMN status TEXT NOT NULL DEFAULT 'pending'
    CHECK (status IN ('pending', 'reviewed', 'resolved')),
ADD COLUMN linked_invoice_id UUID REFERENCES invoices(id) ON DELETE SET NULL;

CREATE INDEX idx_unmatched_payments_status ON unmatched_payments (status);
CREATE INDEX idx_unmatched_payments_linked_invoice_id ON unmatched_payments (linked_invoice_id);
