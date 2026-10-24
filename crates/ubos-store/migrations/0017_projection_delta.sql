-- PH-2 step 3c (DEC-100): projection rows (DSN-DVM-420, BPA 1.0-rc9 STD-TIME-021).
-- One row per (branch, projection, declaration version, commit, group, valid time): the
-- fold of the entries that commit appended on the branch for that group and valid time.
-- A read adds the rows up to the branch's mark (kernel_meta `projection_mark:...`) to the
-- fold of the entries after it, so a gap in the maintenance never changes a result.
-- Business classes create no DDL (AR-018): every projection lives in this one table.
CREATE TABLE projection_delta (
  tenant_id   uuid    NOT NULL,
  vae_id      uuid    NOT NULL,
  branch_id   bigint  NOT NULL,
  projection  text    NOT NULL,
  version     text    NOT NULL,
  cseq        bigint  NOT NULL,
  group_key   text    NOT NULL,
  valid_at    text    NOT NULL,
  delta       bytea   NOT NULL,
  PRIMARY KEY (tenant_id, vae_id, branch_id, projection, version, cseq, group_key, valid_at));
ALTER TABLE projection_delta ENABLE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON projection_delta USING (tenant_id = nullif(current_setting('ubos.tenant', true), '')::uuid) WITH CHECK (tenant_id = nullif(current_setting('ubos.tenant', true), '')::uuid);
GRANT SELECT, INSERT, UPDATE, DELETE, TRUNCATE ON projection_delta TO ubos_app;
