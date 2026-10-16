-- Node configuration versions, PH-1 step 16a (DSN-NOD-603; FR-OPS-091, -093; DEC-077).
--
-- Each applied version of a node's `ubos.toml`: its content hash, the author and source
-- of the change, the document (references only, never secret values), the diff against
-- the running version and the keys that wait for a restart. A platform table of the owner
-- role, outside the tenant schemas.

CREATE TABLE node_config (
  seq               bigserial   PRIMARY KEY,
  node_id           text        NOT NULL,
  applied_at        timestamptz NOT NULL DEFAULT now(),
  hash              text        NOT NULL,
  author            text        NOT NULL,
  source            text        NOT NULL,
  document          text        NOT NULL,
  diff              jsonb       NOT NULL,
  restart_required  jsonb       NOT NULL);

CREATE INDEX node_config_node ON node_config (node_id, seq);
