-- Add migration script here
CREATE TABLE tokens (
    id BIGSERIAL PRIMARY KEY,
    token TEXT NOT NULL
);

CREATE TABLE invoice_payment_data (
    primary_key                      BIGSERIAL     PRIMARY KEY,
    invoice_id              TEXT          NOT NULL,
    status_update_callback  TEXT          NOT NULL,
    success_callback        TEXT          NOT NULL,
    method                  TEXT          NOT NULL,
    generated_address       TEXT          NOT NULL,
    address_index           TEXT          NOT NULL,
    amount                  TEXT          NOT NULL,
    created_at              TIMESTAMPTZ   NOT NULL  DEFAULT NOW(),
    completed_at            TIMESTAMPTZ   NULL,
    sweeped_at              TIMESTAMPTZ  NULL, 
);


CREATE TABLE callback_headers (
    id bigserial PRIMARY KEY,
    invoice_id text NOT NULL,
    key text NOT NULL,
    value text NOT NULL
);
