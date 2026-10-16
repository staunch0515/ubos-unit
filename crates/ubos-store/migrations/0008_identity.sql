-- Local identity of the node, PH-1 step 14c (DSN-NOD-101, -105, -106, -108, -109, -110,
-- -111, -150, -153; DEC-075).
--
-- * `identity_record`: the records of a tenant's identity by kind and key — principals
--   (`principal:<did>`), credentials (`credential:<username>`: Argon2id hash, sealed TOTP
--   secret, failure counters), sources (`source:<ip>`: failure counters), sessions
--   (`session:<token hash>` and `session_id:<id>`), API keys (`api_key:<key hash>` and
--   `api_key_id:<id>`), principal key histories (`principal_key:<did>`) and policies
--   (`policy:session`). Secrets are never stored in clear: passwords as PHC strings, TOTP
--   secrets sealed under a key derived from the tenant KEK, tokens and API keys as hashes.
-- * `node_security_log`: the node's hash-chained security log per tenant (DSN-NOD-111).
-- Both are tenant tables under row-level security like the kernel tables.

CREATE TABLE identity_record (
  tenant_id    uuid        NOT NULL,
  kind         text        NOT NULL,
  key          text        NOT NULL,
  body         jsonb       NOT NULL,
  updated_at   timestamptz NOT NULL DEFAULT now(),
  PRIMARY KEY (tenant_id, kind, key));

CREATE TABLE node_security_log (
  tenant_id    uuid        NOT NULL,
  seq          bigint      NOT NULL,
  entry        jsonb       NOT NULL,
  PRIMARY KEY (tenant_id, seq));

ALTER TABLE identity_record ENABLE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON identity_record USING (tenant_id = nullif(current_setting('ubos.tenant', true), '')::uuid) WITH CHECK (tenant_id = nullif(current_setting('ubos.tenant', true), '')::uuid);
GRANT SELECT, INSERT, UPDATE, DELETE, TRUNCATE ON identity_record TO ubos_app;
ALTER TABLE node_security_log ENABLE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON node_security_log USING (tenant_id = nullif(current_setting('ubos.tenant', true), '')::uuid) WITH CHECK (tenant_id = nullif(current_setting('ubos.tenant', true), '')::uuid);
GRANT SELECT, INSERT, UPDATE, DELETE, TRUNCATE ON node_security_log TO ubos_app;
