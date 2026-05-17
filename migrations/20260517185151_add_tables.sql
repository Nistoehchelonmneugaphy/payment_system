-- Add migration script here
CREATE TABLE tokens (
    id BIGSERIAL PRIMARY KEY,
    token TEXT NOT NULL
);

CREATE TABLE invoice_payment_data (
    id                      BIGSERIAL     PRIMARY KEY,
    invoice_id              TEXT          NOT NULL,
    status_update_callback  TEXT          NOT NULL,
    success_callback        TEXT          NOT NULL,
    method                  TEXT          NOT NULL,
    address                 TEXT          NOT NULL,
    amount                  TEXT          NOT NULL,
    created_at              TIMESTAMPTZ   NOT NULL  DEFAULT NOW()
    completed_at            TIMESTAMPTZ   NULL
);

