-- Files: resumable uploads and content-addressed, chunked files, PH-1 step 17a
-- (DSN-NOD-320, -321, -327; FR-FILE-011, -012; DEC-078).
--
-- * `upload`: a tus-compatible upload in progress: the declared length, the bytes received,
--   the bytes after the last chunk boundary (`carry`, at most one chunk) and the chunks cut
--   so far. The chunks themselves are sealed into the tenant's blob store as they are cut,
--   so a resumed upload continues from `received`.
-- * `file`: a completed file: the manifest of STD-OBJ-008 (`{chunks, total_size,
--   media_type}`), whose content identifier is the file identifier, the chunk sizes and
--   the SHA-256 of the content. Chunks are deduplicated within a tenant only (the blob
--   path carries the tenant).

CREATE TABLE upload (
  tenant_id  uuid        NOT NULL,
  upload_id  uuid        NOT NULL,
  principal  text        NOT NULL,
  length     bigint      NOT NULL,
  received   bigint      NOT NULL DEFAULT 0,
  media_type text,
  name       text,
  carry      bytea       NOT NULL DEFAULT ''::bytea,
  chunks     jsonb       NOT NULL DEFAULT '[]'::jsonb,
  file_id    text,
  created_at timestamptz NOT NULL DEFAULT now(),
  expires_at timestamptz NOT NULL,
  PRIMARY KEY (tenant_id, upload_id));

CREATE TABLE file (
  tenant_id  uuid        NOT NULL,
  file_id    text        NOT NULL,
  manifest   jsonb       NOT NULL,
  sizes      jsonb       NOT NULL,
  size       bigint      NOT NULL,
  sha256     text        NOT NULL,
  media_type text,
  created_at timestamptz NOT NULL DEFAULT now(),
  PRIMARY KEY (tenant_id, file_id));

DO $$
DECLARE
  t text;
BEGIN
  FOREACH t IN ARRAY ARRAY['upload', 'file'] LOOP
    EXECUTE format('ALTER TABLE %I ENABLE ROW LEVEL SECURITY', t);
    EXECUTE format('CREATE POLICY tenant_isolation ON %I USING (tenant_id = nullif(current_setting(''ubos.tenant'', true), '''')::uuid) WITH CHECK (tenant_id = nullif(current_setting(''ubos.tenant'', true), '''')::uuid)', t);
  END LOOP;
END $$;

GRANT SELECT, INSERT, UPDATE, DELETE ON upload, file TO ubos_app;
