-- Metadata tables of the kernel, PH-1 step 4d-2 (UBS-SYS-DVM-04 §2 and §5, DEC-054).
--
-- The branch, tag, process, change-set, derivation, outbox, idempotency and timer records
-- that the engine kept under keys of `kernel_meta` since steps 4b and 4c get their tables:
--   * branch metadata as columns of `branch_ref`;
--   * `tag`, `process_record`, `change_set`;
--   * derivation records as chunks (their identifier is their content ID) with one
--     `derivation_edge` row per input, written in the flush (DSN-DVM-533);
--   * `outbox` rows and `idempotency_record` rows written in the flush (DSN-DVM-530, -534),
--     acknowledgement as `released_at`;
--   * `timer` rows for the future segment starts that emit BecameEffective (STD-TIME-050).
-- `kernel_meta` stays for adapter settings.

ALTER TABLE branch_ref
  ALTER COLUMN kind TYPE text USING kind::text,
  ALTER COLUMN kind SET DEFAULT 'main',
  ADD COLUMN created_at   timestamptz,
  ADD COLUMN active_at    timestamptz,
  ADD COLUMN parent_line  jsonb,                 -- {branch, mode, pin}
  ADD COLUMN created_from bytea;                 -- commit ID the branch was created from

CREATE TABLE tag (
  tenant_id  uuid    NOT NULL,
  vae_id     uuid    NOT NULL,
  name       text    NOT NULL,
  commit_id  bytea   NOT NULL,
  branch     text    NOT NULL,
  signer     text    NOT NULL,
  signature  text    NOT NULL,
  tombstoned boolean NOT NULL DEFAULT false,
  PRIMARY KEY (tenant_id, vae_id, name));

CREATE TABLE process_record (
  tenant_id       uuid NOT NULL,
  process_id      uuid NOT NULL,
  vae_id          uuid NOT NULL,
  seq             bigint GENERATED ALWAYS AS IDENTITY,
  kind            text NOT NULL,
  state           text NOT NULL,
  initiator       text NOT NULL,
  trigger_kind    text NOT NULL,
  trigger_ref     text,
  parent_process  uuid,
  bulk            uuid,
  idempotency_key text,
  effects         integer NOT NULL DEFAULT 0,
  commits         jsonb NOT NULL DEFAULT '[]',   -- [[branch, cseq], …]
  bulk_state      jsonb,                         -- STD-TXN-070: mode, staging, children, attempts
  started_at      timestamptz NOT NULL DEFAULT now(),
  PRIMARY KEY (tenant_id, vae_id, process_id));
CREATE INDEX process_record_order ON process_record (tenant_id, vae_id, seq);

CREATE TABLE change_set (
  tenant_id     uuid NOT NULL,
  vae_id        uuid NOT NULL,
  change_set_id uuid NOT NULL,
  target        text NOT NULL,
  merge_commit  bytea,
  PRIMARY KEY (tenant_id, vae_id, change_set_id));

DROP TABLE derivation_edge;
CREATE TABLE derivation_edge (
  tenant_id      uuid        NOT NULL,
  vae_id         uuid        NOT NULL,
  binding_id     bytea       NOT NULL,         -- the derivation record (a chunk)
  output_version bytea       NOT NULL,         -- derived version or entry hash
  input_object   uuid        NOT NULL,
  input_version  bytea       NOT NULL,
  valid_at       timestamptz NOT NULL,
  granularity    smallint    NOT NULL,
  PRIMARY KEY (tenant_id, vae_id, binding_id, output_version, input_object, input_version, valid_at));
CREATE INDEX derivation_out ON derivation_edge (tenant_id, vae_id, output_version);
CREATE INDEX derivation_in ON derivation_edge (tenant_id, vae_id, input_version);

ALTER TABLE outbox ADD COLUMN effect_id text NOT NULL;
CREATE UNIQUE INDEX outbox_effect ON outbox (tenant_id, vae_id, effect_id);

CREATE TABLE timer (
  tenant_id  uuid        NOT NULL,
  vae_id     uuid        NOT NULL,
  id         uuid        NOT NULL,
  due_at     timestamptz NOT NULL,
  target     uuid        NOT NULL,             -- object
  step       jsonb       NOT NULL,             -- {branch, from, version, class}
  attempt    integer     NOT NULL DEFAULT 0,
  claimed_by text, claim_until timestamptz, fired_at timestamptz,
  PRIMARY KEY (tenant_id, id));
CREATE INDEX timer_due ON timer (tenant_id, vae_id, due_at) WHERE fired_at IS NULL;

DO $$
DECLARE
  t text;
BEGIN
  FOREACH t IN ARRAY ARRAY['tag', 'process_record', 'change_set', 'derivation_edge', 'timer'] LOOP
    EXECUTE format('ALTER TABLE %I ENABLE ROW LEVEL SECURITY', t);
    EXECUTE format('CREATE POLICY tenant_isolation ON %I USING (tenant_id = nullif(current_setting(''ubos.tenant'', true), '''')::uuid) WITH CHECK (tenant_id = nullif(current_setting(''ubos.tenant'', true), '''')::uuid)', t);
  END LOOP;
END $$;

GRANT SELECT, INSERT, UPDATE, DELETE, TRUNCATE ON ALL TABLES IN SCHEMA public TO ubos_app;
GRANT USAGE, SELECT, UPDATE ON ALL SEQUENCES IN SCHEMA public TO ubos_app;
REVOKE UPDATE, DELETE ON ledger_entry FROM ubos_app;
REVOKE ALL ON _sqlx_migrations FROM ubos_app;
