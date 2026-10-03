-- Host service tables of the in-process node host, PH-1 step 7a (CTR-002, DSN-NOD-130,
-- -131, -134, -201, -202, -220, -320, -401, -402; DEC-060).
--
-- * `tenant` and `vae`: the tenant registry. The tenant's key-encryption key (KEK) is
--   stored wrapped by the root KEK of the node (XChaCha20-Poly1305, DSN-NOD-131); data keys
--   and node signing keys are derived from it (HKDF) and never stored.
-- * `node_key`: the public key history of a VAE's node signing key (STD-PROOF-010).
-- * `blob`: the blob service (chunks above the inline size, DSN-DVM-003, -320).
-- * `secret`: secrets sealed under a tenant data key, addressed by handle (DSN-NOD-130).
-- * `event_log`: effects released from the outbox (DSN-NOD-201, -202).
-- * `job`: jobs with leases and retry counts (DSN-NOD-220, -221, -223).
-- The registry tables are read by the node as the owner role; the others are tenant tables
-- under row-level security like the kernel tables.

CREATE TABLE tenant (
  tenant_id    uuid        PRIMARY KEY,
  slug         text        NOT NULL UNIQUE,
  state        text        NOT NULL DEFAULT 'active',
  kek_wrapped  bytea       NOT NULL,
  kek_nonce    bytea       NOT NULL,
  root_key_id  integer     NOT NULL DEFAULT 1,
  data_key     integer     NOT NULL DEFAULT 1,          -- current data key generation
  created_at   timestamptz NOT NULL DEFAULT now());

CREATE TABLE vae (
  tenant_id    uuid        NOT NULL REFERENCES tenant (tenant_id),
  vae_id       uuid        NOT NULL,
  authority    text        NOT NULL,                    -- STD-ADDR-001
  created_at   timestamptz NOT NULL DEFAULT now(),
  PRIMARY KEY (tenant_id, vae_id),
  UNIQUE (tenant_id, authority));

CREATE TABLE node_key (
  tenant_id    uuid        NOT NULL,
  vae_id       uuid        NOT NULL,
  n            integer     NOT NULL,
  public_key   text        NOT NULL,                    -- base64url
  valid_from   text,                                    -- instants as canonical text
  valid_to     text,
  PRIMARY KEY (tenant_id, vae_id, n));

CREATE TABLE blob (
  tenant_id    uuid        NOT NULL,
  path         text        NOT NULL,
  bytes        bytea       NOT NULL,
  created_at   timestamptz NOT NULL DEFAULT now(),
  PRIMARY KEY (tenant_id, path));

CREATE TABLE secret (
  tenant_id    uuid        NOT NULL,
  handle       text        NOT NULL,
  key_id       integer     NOT NULL,
  nonce        bytea       NOT NULL,
  sealed       bytea       NOT NULL,
  created_at   timestamptz NOT NULL DEFAULT now(),
  PRIMARY KEY (tenant_id, handle));

CREATE TABLE event_log (
  tenant_id    uuid        NOT NULL,
  seq          bigint      GENERATED ALWAYS AS IDENTITY,
  vae_id       uuid        NOT NULL,
  effect_id    text        NOT NULL,
  type         text        NOT NULL,
  branch       text        NOT NULL,
  cseq         bigint,
  object_id    text,
  payload      jsonb,
  released_at  timestamptz NOT NULL DEFAULT now(),
  PRIMARY KEY (tenant_id, seq),
  UNIQUE (tenant_id, vae_id, effect_id));

CREATE TABLE job (
  tenant_id    uuid        NOT NULL,
  job_id       uuid        NOT NULL,
  vae_id       uuid        NOT NULL,
  kind         text        NOT NULL,
  payload      jsonb       NOT NULL,
  state        text        NOT NULL DEFAULT 'queued',   -- queued | running | done | failed | poison
  attempts     integer     NOT NULL DEFAULT 0,
  max_attempts integer     NOT NULL DEFAULT 3,
  worker       text,
  lease_until  timestamptz,
  result       jsonb,
  error        text,
  created_at   timestamptz NOT NULL DEFAULT now(),
  updated_at   timestamptz NOT NULL DEFAULT now(),
  PRIMARY KEY (tenant_id, job_id));
CREATE INDEX job_ready ON job (tenant_id, vae_id, state, lease_until);

DO $$
DECLARE
  t text;
BEGIN
  FOREACH t IN ARRAY ARRAY['node_key', 'blob', 'secret', 'event_log', 'job'] LOOP
    EXECUTE format('ALTER TABLE %I ENABLE ROW LEVEL SECURITY', t);
    EXECUTE format('CREATE POLICY tenant_isolation ON %I USING (tenant_id = nullif(current_setting(''ubos.tenant'', true), '''')::uuid) WITH CHECK (tenant_id = nullif(current_setting(''ubos.tenant'', true), '''')::uuid)', t);
  END LOOP;
END $$;

GRANT SELECT, INSERT, UPDATE, DELETE, TRUNCATE ON ALL TABLES IN SCHEMA public TO ubos_app;
GRANT USAGE, SELECT, UPDATE ON ALL SEQUENCES IN SCHEMA public TO ubos_app;
REVOKE ALL ON tenant, vae FROM ubos_app;
