-- Deliveries and dead letters, PH-1 step 15b (DSN-NOD-204, -209, -223; FR-EVT-051, -052;
-- DEC-076): a handler delivery is a job of kind `delivery`, and a job that exhausts its
-- retry policy is `dead` (a dead letter).
--
-- * `not_before`: the earliest next attempt (exponential backoff with jitter).
-- * `dedupe_key`: one job per key and VAE (one delivery per subscription and event).
-- * `annotations`: operators' notes on a job (`[{at, by, text}]`); payloads stay as they are.
-- * states: queued | running | done | dead | cancelled (`poison` of gate 1a reads as dead).

ALTER TABLE job ADD COLUMN not_before timestamptz NOT NULL DEFAULT now();
ALTER TABLE job ADD COLUMN dedupe_key text;
ALTER TABLE job ADD COLUMN annotations jsonb NOT NULL DEFAULT '[]'::jsonb;
UPDATE job SET state = 'dead' WHERE state = 'poison';
CREATE UNIQUE INDEX job_dedupe ON job (tenant_id, vae_id, dedupe_key) WHERE dedupe_key IS NOT NULL;
CREATE INDEX job_claim ON job (tenant_id, vae_id, kind, state, not_before);
