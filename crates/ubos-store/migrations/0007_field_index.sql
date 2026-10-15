-- PH-1 step 13a (DEC-073): declared field indexes (DSN-DVM-902, FR-ANL-014).
-- One row per (object, indexed field) and head version visible in [cseq_from, cseq_to):
-- the value key is the order-preserving encoding of the field value per type
-- (ubos-types `values::index_key`), so equality and range predicates of criteria queries
-- read a key range instead of scanning the class. Business classes create no DDL
-- (AR-018): every index of every class lives in this one table.
CREATE TABLE field_index (
  tenant_id   uuid    NOT NULL,
  vae_id      uuid    NOT NULL,
  branch_id   bigint  NOT NULL,
  class_name  text    NOT NULL,
  field       text    NOT NULL,
  value_key   bytea   NOT NULL,
  object_id   uuid    NOT NULL,
  cseq_from   bigint  NOT NULL,
  cseq_to     bigint  NOT NULL,
  PRIMARY KEY (tenant_id, vae_id, branch_id, class_name, field, value_key, object_id, cseq_from));
CREATE INDEX field_index_object ON field_index (tenant_id, vae_id, branch_id, object_id, cseq_to);
ALTER TABLE field_index ENABLE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON field_index USING (tenant_id = nullif(current_setting('ubos.tenant', true), '')::uuid) WITH CHECK (tenant_id = nullif(current_setting('ubos.tenant', true), '')::uuid);
GRANT SELECT, INSERT, UPDATE, DELETE, TRUNCATE ON field_index TO ubos_app;
