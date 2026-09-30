-- Kernel schema of the PostgreSQL adapter, step 3a (UBS-SYS-DVM-04, DSN-DVM-001, DEC-047).
--
-- Tables follow UBS-SYS-DVM-04: tenant_id first in every key, content IDs as 32-byte
-- bytea, branch_id surrogates, cseq intervals. Transition columns (DEC-047), replaced when
-- the engine reaches production form in PH-1 step 4:
--   * commits carry the engine's integer identifier commit_no as key; commit_id is the
--     content ID of the canonical commit body;
--   * class and object identifiers are text (class names, object IDs as given);
--   * valid times are text (ISO dates or instants, as in the canonical form);
--   * object_state holds the engine's per-object state (class, kind, slug, head,
--     timeline) with cseq intervals; head_index and temporal_index are maintained from it
--     in the same flush;
--   * kernel_meta holds adapter settings (class declarations, clock).
-- Partitioning and row-level security follow in migration 0002 (step 3b).

CREATE TABLE kernel_meta (
  tenant_id uuid NOT NULL, vae_id uuid NOT NULL, key text NOT NULL, value jsonb NOT NULL,
  PRIMARY KEY (tenant_id, vae_id, key));

CREATE TABLE branch_ref (
  tenant_id     uuid     NOT NULL,
  vae_id        uuid     NOT NULL,
  branch_id     bigint   NOT NULL,
  name          text     NOT NULL,
  kind          smallint NOT NULL DEFAULT 0,
  state         smallint NOT NULL DEFAULT 0,
  head_no       bigint   NOT NULL,          -- transition: commit_no of the head
  head_cseq     bigint   NOT NULL,
  parent_branch bigint,
  fork_cseq     bigint,
  depth         smallint NOT NULL,
  leader_node   text,
  leader_lease_until timestamptz,
  PRIMARY KEY (tenant_id, vae_id, branch_id),
  UNIQUE (tenant_id, vae_id, name));

CREATE SEQUENCE commit_no_seq;             -- identifiers, not ordinals: gaps allowed

CREATE TABLE commit (
  tenant_id  uuid   NOT NULL,
  vae_id     uuid   NOT NULL,
  commit_no  bigint NOT NULL,               -- transition key
  commit_id  bytea  NOT NULL,               -- content ID of body
  branch_id  bigint NOT NULL,
  cseq       bigint NOT NULL,
  state_root bytea  NOT NULL,
  body       bytea  NOT NULL,               -- canonical commit record
  PRIMARY KEY (tenant_id, vae_id, commit_no),
  UNIQUE (tenant_id, vae_id, branch_id, cseq));

CREATE TABLE commit_parent (
  tenant_id uuid NOT NULL, vae_id uuid NOT NULL, commit_no bigint NOT NULL, ord smallint NOT NULL,
  parent_no bigint NOT NULL,
  PRIMARY KEY (tenant_id, vae_id, commit_no, ord));

CREATE TABLE chunk (
  tenant_id     uuid     NOT NULL,
  content_id    bytea    NOT NULL,
  placement     smallint NOT NULL,          -- 0 inline, 1 object store, 2 cold tier, 3 disposed
  size_logical  integer  NOT NULL,
  body          bytea,
  disposal_cert bytea,
  PRIMARY KEY (tenant_id, content_id));

CREATE TABLE object_version (
  tenant_id  uuid  NOT NULL,
  version_id bytea NOT NULL,
  object_id  text  NOT NULL,                -- transition: text
  class_name text  NOT NULL,                -- transition: class name for class_id
  envelope   bytea NOT NULL,                -- canonical stored record (physical form: 3b)
  chunk_ids  bytea[],
  flags      integer NOT NULL DEFAULT 0,
  PRIMARY KEY (tenant_id, version_id));

CREATE TABLE ledger_entry (
  tenant_id    uuid   NOT NULL,
  vae_id       uuid   NOT NULL,
  branch_id    bigint NOT NULL,
  ledger_class text   NOT NULL,             -- transition: class name
  ledger_key   bytea  NOT NULL,             -- hash of the canonical key
  seq          bigint NOT NULL,
  entry_id     text   NOT NULL,             -- transition: identifier as given
  cseq         bigint NOT NULL,
  idx          integer NOT NULL,            -- position in the commit (append order)
  prev_hash    bytea  NOT NULL,             -- hash chain per key
  entry_hash   bytea  NOT NULL,
  body         bytea  NOT NULL,             -- canonical entry record
  PRIMARY KEY (tenant_id, vae_id, branch_id, ledger_class, ledger_key, seq));
CREATE INDEX ledger_entry_order ON ledger_entry (tenant_id, vae_id, branch_id, cseq, idx);
CREATE INDEX ledger_entry_id ON ledger_entry (tenant_id, vae_id, branch_id, entry_id);

CREATE TABLE ledger_sequence (
  tenant_id uuid NOT NULL, vae_id uuid NOT NULL, branch_id bigint NOT NULL,
  ledger_class text NOT NULL, ledger_key bytea NOT NULL,
  next_seq bigint NOT NULL, last_hash bytea NOT NULL,
  PRIMARY KEY (tenant_id, vae_id, branch_id, ledger_class, ledger_key));

CREATE TABLE merkle_node (
  tenant_id uuid NOT NULL, node_id bytea NOT NULL, level smallint NOT NULL, body bytea NOT NULL,
  PRIMARY KEY (tenant_id, node_id));

CREATE TABLE object_state (                 -- transition table (see header)
  tenant_id uuid NOT NULL, vae_id uuid NOT NULL, branch_id bigint NOT NULL,
  object_id text NOT NULL, cseq_from bigint NOT NULL, cseq_to bigint NOT NULL,
  class_name text NOT NULL, state bytea,    -- canonical state; NULL: removed on the branch
  PRIMARY KEY (tenant_id, vae_id, branch_id, object_id, cseq_from));
CREATE INDEX object_state_class ON object_state (tenant_id, vae_id, branch_id, class_name, cseq_to);

CREATE TABLE head_index (
  tenant_id uuid NOT NULL, vae_id uuid NOT NULL, branch_id bigint NOT NULL, object_id text NOT NULL,
  version_id bytea, class_name text NOT NULL, hidden boolean NOT NULL,
  PRIMARY KEY (tenant_id, vae_id, branch_id, object_id));

CREATE TABLE temporal_index (
  tenant_id uuid NOT NULL, vae_id uuid NOT NULL, branch_id bigint NOT NULL, object_id text NOT NULL,
  valid_from text NOT NULL, valid_to text,  -- transition: text; NULL is open
  cseq_from bigint NOT NULL, cseq_to bigint NOT NULL,
  version_id bytea NOT NULL, class_name text NOT NULL,
  PRIMARY KEY (tenant_id, vae_id, branch_id, object_id, valid_from, cseq_from));
CREATE INDEX temporal_class ON temporal_index (tenant_id, vae_id, branch_id, class_name, cseq_to);

CREATE TABLE derivation_edge (
  tenant_id uuid NOT NULL, output_version bytea NOT NULL, input_version bytea NOT NULL,
  binding_id bytea, valid_at text,
  PRIMARY KEY (tenant_id, input_version, output_version));
CREATE INDEX derivation_out ON derivation_edge (tenant_id, output_version);

CREATE TABLE idempotency_record (
  tenant_id uuid NOT NULL, scope text NOT NULL, key text NOT NULL, request_hash bytea NOT NULL,
  outcome bytea, commit_no bigint, expires_at timestamptz,
  PRIMARY KEY (tenant_id, scope, key));

CREATE TABLE outbox (
  tenant_id uuid NOT NULL, id bigint GENERATED ALWAYS AS IDENTITY, vae_id uuid NOT NULL,
  branch_id bigint NOT NULL, cseq bigint NOT NULL, idx integer NOT NULL, kind smallint NOT NULL,
  aggregate_key bytea, payload bytea NOT NULL,
  claimed_by text, claim_until timestamptz, attempts integer NOT NULL DEFAULT 0, released_at timestamptz,
  PRIMARY KEY (tenant_id, id));
CREATE INDEX outbox_ready ON outbox (claim_until) WHERE released_at IS NULL;
