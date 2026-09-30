-- Tenancy of the PostgreSQL adapter, step 3b (DSN-DVM-010, UBS-SYS-DVM-04 §1, DEC-048).
--
-- * The large tables are hash-partitioned by tenant into 64 partitions.
-- * Every table has row-level security keyed on the session setting `ubos.tenant`; a
--   session without it sees and writes no rows.
-- * The adapter works as role `ubos_app` (never the owner, so the policies apply); ledger
--   entries cannot be updated or deleted (LEDG.IMMUTABLE_ENTRY).
-- * Range index over commit ordinals of `temporal_index`; the valid-period range follows
--   when valid times become timestamptz in step 4 (DEC-047 transition layout).

CREATE EXTENSION IF NOT EXISTS btree_gist;

DO $$
DECLARE
  t text;
  i int;
BEGIN
  FOREACH t IN ARRAY ARRAY['chunk', 'object_version', 'ledger_entry', 'temporal_index', 'merkle_node'] LOOP
    EXECUTE format('ALTER TABLE %I RENAME TO %I', t, t || '_unpartitioned');
    EXECUTE format('CREATE TABLE %I (LIKE %I INCLUDING DEFAULTS INCLUDING CONSTRAINTS) PARTITION BY HASH (tenant_id)',
                   t, t || '_unpartitioned');
    FOR i IN 0..63 LOOP
      EXECUTE format('CREATE TABLE %I PARTITION OF %I FOR VALUES WITH (MODULUS 64, REMAINDER %s)',
                     t || '_p' || lpad(i::text, 2, '0'), t, i);
    END LOOP;
    EXECUTE format('INSERT INTO %I SELECT * FROM %I', t, t || '_unpartitioned');
    EXECUTE format('DROP TABLE %I', t || '_unpartitioned');
  END LOOP;
END $$;

ALTER TABLE chunk ADD PRIMARY KEY (tenant_id, content_id);
ALTER TABLE object_version ADD PRIMARY KEY (tenant_id, version_id);
ALTER TABLE ledger_entry ADD PRIMARY KEY (tenant_id, vae_id, branch_id, ledger_class, ledger_key, seq);
CREATE INDEX ledger_entry_order ON ledger_entry (tenant_id, vae_id, branch_id, cseq, idx);
CREATE INDEX ledger_entry_id ON ledger_entry (tenant_id, vae_id, branch_id, entry_id);
ALTER TABLE temporal_index ADD PRIMARY KEY (tenant_id, vae_id, branch_id, object_id, valid_from, cseq_from);
CREATE INDEX temporal_class ON temporal_index (tenant_id, vae_id, branch_id, class_name, cseq_to);
CREATE INDEX temporal_cseq ON temporal_index USING gist (tenant_id, vae_id, branch_id, int8range(cseq_from, cseq_to));
ALTER TABLE merkle_node ADD PRIMARY KEY (tenant_id, node_id);

-- Ledger entries are immutable (STD-KIND-020).
CREATE FUNCTION ledger_entry_immutable() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
  RAISE EXCEPTION 'LEDG.IMMUTABLE_ENTRY' USING ERRCODE = 'check_violation';
END $$;
CREATE TRIGGER ledger_entry_immutable BEFORE UPDATE OR DELETE ON ledger_entry
  FOR EACH ROW EXECUTE FUNCTION ledger_entry_immutable();

-- Row-level security on every tenant table.
DO $$
DECLARE
  t text;
BEGIN
  FOREACH t IN ARRAY ARRAY['kernel_meta', 'branch_ref', 'commit', 'commit_parent', 'chunk',
      'object_version', 'ledger_entry', 'ledger_sequence', 'merkle_node', 'object_state',
      'head_index', 'temporal_index', 'derivation_edge', 'idempotency_record', 'outbox'] LOOP
    EXECUTE format('ALTER TABLE %I ENABLE ROW LEVEL SECURITY', t);
    EXECUTE format('CREATE POLICY tenant_isolation ON %I USING (tenant_id = nullif(current_setting(''ubos.tenant'', true), '''')::uuid) WITH CHECK (tenant_id = nullif(current_setting(''ubos.tenant'', true), '''')::uuid)', t);
  END LOOP;
END $$;

-- Application role (cluster-wide, created once).
DO $$
BEGIN
  IF NOT EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'ubos_app') THEN
    CREATE ROLE ubos_app NOLOGIN;
  END IF;
END $$;
GRANT USAGE ON SCHEMA public TO ubos_app;
GRANT SELECT, INSERT, UPDATE, DELETE, TRUNCATE ON ALL TABLES IN SCHEMA public TO ubos_app;
GRANT USAGE, SELECT, UPDATE ON ALL SEQUENCES IN SCHEMA public TO ubos_app;
REVOKE UPDATE, DELETE ON ledger_entry FROM ubos_app;
REVOKE ALL ON _sqlx_migrations FROM ubos_app;
