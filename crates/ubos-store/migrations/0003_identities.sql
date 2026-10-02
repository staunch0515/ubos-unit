-- Production identities of the kernel schema, PH-1 step 4d-1 (UBS-SYS-DVM-04, DEC-053).
--
-- Replaces the transition layout of DEC-047:
--   * commits are keyed by their content ID within the VAE (the canonical record names
--     the VAE); parents are stored by content ID; the engine's integer `commit_no` stays as
--     an internal handle (a surrogate like `branch_id`); `tx_time`, `process_id` and
--     `author` are columns;
--   * object and entry identifiers are `uuid`; valid times are `timestamptz` in UTC with
--     `'infinity'` for an open end, and each index row records the granularity of its
--     class (0 date, 1 instant) so that the engine's canonical text is reproduced exactly;
--   * versions carry their class version, kind and valid period as columns;
--   * `object_state` is gone: the engine's state of an object at a commit ordinal is the
--     `head_index` row visible there (head version, kind, slug, hidden) and the
--     `temporal_index` segments visible there; both are rebuilt from the write sets the
--     commit records carry (`objects`), which the verifier uses;
--   * `temporal_index` gets the valid-period range index promised in migration 0002;
--   * idempotency records point at commits by content ID.
-- Class identifiers stay class names with their declaration's content ID (`class_version`):
-- the standard identifies a class by name within a VAE (DEC-053).
-- The transition layout held no production data; the kernel starts empty.

DROP TABLE IF EXISTS object_state;
DROP TABLE IF EXISTS head_index;
DROP TABLE IF EXISTS temporal_index;
DROP TABLE IF EXISTS commit_parent;
DROP TABLE IF EXISTS commit;
DROP TABLE IF EXISTS object_version;
DROP TABLE IF EXISTS ledger_entry;
DROP TABLE IF EXISTS idempotency_record;
TRUNCATE kernel_meta, branch_ref, chunk, ledger_sequence, merkle_node, derivation_edge, outbox;

ALTER TABLE branch_ref ADD COLUMN head_commit bytea;        -- content ID of the head
ALTER TABLE branch_ref ALTER COLUMN head_no SET DEFAULT 0;  -- internal handle of the head

CREATE TABLE commit (
  tenant_id  uuid   NOT NULL,
  commit_id  bytea  NOT NULL,               -- content ID of the canonical commit record
  vae_id     uuid   NOT NULL,
  branch_id  bigint NOT NULL,
  cseq       bigint NOT NULL,
  commit_no  bigint NOT NULL,               -- internal handle (not an ordinal)
  tx_time    timestamptz,                   -- NULL for a genesis commit
  state_root bytea  NOT NULL,
  process_id uuid,                          -- explicit processes only (DEC-051)
  author     text,                          -- initiator did:ubos
  body       bytea  NOT NULL,               -- the engine's commit record
  PRIMARY KEY (tenant_id, vae_id, commit_id),
  UNIQUE (tenant_id, vae_id, branch_id, cseq),
  UNIQUE (tenant_id, vae_id, commit_no));
CREATE INDEX commit_time ON commit (tenant_id, vae_id, branch_id, tx_time);

CREATE TABLE commit_parent (
  tenant_id uuid NOT NULL, vae_id uuid NOT NULL, commit_id bytea NOT NULL, ord smallint NOT NULL,
  parent_id bytea NOT NULL,
  PRIMARY KEY (tenant_id, vae_id, commit_id, ord));

CREATE TABLE object_version (
  tenant_id     uuid        NOT NULL,
  version_id    bytea       NOT NULL,       -- content ID of the envelope
  object_id     uuid        NOT NULL,
  vae_id        uuid        NOT NULL,       -- VAE of the first writer
  class_name    text        NOT NULL,
  class_version bytea       NOT NULL,       -- content ID of the class declaration
  kind          smallint    NOT NULL,       -- 1 definition, 2 ledger, 3 system, 4 raw_document, 5 inherit
  valid_from    timestamptz NOT NULL,
  valid_to      timestamptz NOT NULL,       -- 'infinity' when open
  envelope      bytea       NOT NULL,       -- canonical envelope with body (physical form)
  chunk_ids     bytea[],
  flags         integer     NOT NULL DEFAULT 0
) PARTITION BY HASH (tenant_id);

CREATE TABLE ledger_entry (
  tenant_id    uuid        NOT NULL,
  vae_id       uuid        NOT NULL,
  branch_id    bigint      NOT NULL,
  ledger_class text        NOT NULL,
  ledger_key   bytea       NOT NULL,        -- hash of the canonical key
  seq          bigint      NOT NULL,
  entry_id     uuid        NOT NULL,
  valid_at     timestamptz NOT NULL,
  cseq         bigint      NOT NULL,
  idx          integer     NOT NULL,        -- position in the commit (append order)
  prev_hash    bytea       NOT NULL,        -- hash chain per key
  entry_hash   bytea       NOT NULL,        -- content ID of the entry
  body         bytea       NOT NULL         -- canonical entry record
) PARTITION BY HASH (tenant_id);

CREATE TABLE head_index (
  tenant_id   uuid     NOT NULL,
  vae_id      uuid     NOT NULL,
  branch_id   bigint   NOT NULL,
  object_id   uuid     NOT NULL,
  cseq_from   bigint   NOT NULL,
  cseq_to     bigint   NOT NULL,
  version_id  bytea,                        -- NULL: no version on the branch
  class_name  text     NOT NULL,
  kind        smallint NOT NULL,
  slug        text     NOT NULL,
  granularity smallint NOT NULL,            -- 0 date, 1 instant
  hidden      boolean  NOT NULL,            -- removed on the branch
  PRIMARY KEY (tenant_id, vae_id, branch_id, object_id, cseq_from));
CREATE INDEX head_class ON head_index (tenant_id, vae_id, branch_id, class_name, cseq_to);

CREATE TABLE temporal_index (
  tenant_id   uuid        NOT NULL,
  vae_id      uuid        NOT NULL,
  branch_id   bigint      NOT NULL,
  object_id   uuid        NOT NULL,
  valid_from  timestamptz NOT NULL,
  valid_to    timestamptz NOT NULL,         -- 'infinity' when open
  cseq_from   bigint      NOT NULL,
  cseq_to     bigint      NOT NULL,
  version_id  bytea       NOT NULL,
  class_name  text        NOT NULL,
  granularity smallint    NOT NULL
) PARTITION BY HASH (tenant_id);

CREATE TABLE idempotency_record (
  tenant_id uuid NOT NULL, scope text NOT NULL, key text NOT NULL, request_hash bytea NOT NULL,
  outcome bytea, commit_id bytea, expires_at timestamptz,
  PRIMARY KEY (tenant_id, scope, key));

DO $$
DECLARE
  t text;
  i int;
BEGIN
  FOREACH t IN ARRAY ARRAY['object_version', 'ledger_entry', 'temporal_index'] LOOP
    FOR i IN 0..63 LOOP
      EXECUTE format('CREATE TABLE %I PARTITION OF %I FOR VALUES WITH (MODULUS 64, REMAINDER %s)',
                     t || '_p' || lpad(i::text, 2, '0'), t, i);
    END LOOP;
  END LOOP;
END $$;

ALTER TABLE object_version ADD PRIMARY KEY (tenant_id, version_id);
CREATE INDEX object_version_object ON object_version (tenant_id, object_id);
ALTER TABLE ledger_entry ADD PRIMARY KEY (tenant_id, vae_id, branch_id, ledger_class, ledger_key, seq);
CREATE INDEX ledger_entry_order ON ledger_entry (tenant_id, vae_id, branch_id, cseq, idx);
CREATE INDEX ledger_entry_id ON ledger_entry (tenant_id, vae_id, branch_id, entry_id);
ALTER TABLE temporal_index ADD PRIMARY KEY (tenant_id, vae_id, branch_id, object_id, valid_from, cseq_from);
CREATE INDEX temporal_class ON temporal_index (tenant_id, vae_id, branch_id, class_name, cseq_to);
CREATE INDEX temporal_cseq ON temporal_index USING gist (tenant_id, vae_id, branch_id, int8range(cseq_from, cseq_to));
CREATE INDEX temporal_valid ON temporal_index USING gist
  (tenant_id, vae_id, branch_id, tstzrange(valid_from, valid_to), int8range(cseq_from, cseq_to));

-- Ledger entries are immutable (STD-KIND-020).
CREATE TRIGGER ledger_entry_immutable BEFORE UPDATE OR DELETE ON ledger_entry
  FOR EACH ROW EXECUTE FUNCTION ledger_entry_immutable();

-- Row-level security on the recreated tables (DSN-DVM-010).
DO $$
DECLARE
  t text;
BEGIN
  FOREACH t IN ARRAY ARRAY['commit', 'commit_parent', 'object_version', 'ledger_entry',
      'head_index', 'temporal_index', 'idempotency_record'] LOOP
    EXECUTE format('ALTER TABLE %I ENABLE ROW LEVEL SECURITY', t);
    EXECUTE format('CREATE POLICY tenant_isolation ON %I USING (tenant_id = nullif(current_setting(''ubos.tenant'', true), '''')::uuid) WITH CHECK (tenant_id = nullif(current_setting(''ubos.tenant'', true), '''')::uuid)', t);
  END LOOP;
END $$;

GRANT SELECT, INSERT, UPDATE, DELETE, TRUNCATE ON ALL TABLES IN SCHEMA public TO ubos_app;
GRANT USAGE, SELECT, UPDATE ON ALL SEQUENCES IN SCHEMA public TO ubos_app;
REVOKE UPDATE, DELETE ON ledger_entry FROM ubos_app;
REVOKE ALL ON _sqlx_migrations FROM ubos_app;
