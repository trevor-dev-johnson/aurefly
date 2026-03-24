ALTER TABLE invoices
    ADD COLUMN client_request_id UUID;

ALTER TABLE invoices
    ADD CONSTRAINT invoices_user_id_client_request_id_key
    UNIQUE (user_id, client_request_id);
