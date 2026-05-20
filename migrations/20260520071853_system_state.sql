-- Add migration script here
CREATE TABLE "system_state" (
  "primary_key" bigserial NOT NULL,
  PRIMARY KEY ("primary_key"),
  "name" text NOT NULL,
  "value" text NOT NULL
);