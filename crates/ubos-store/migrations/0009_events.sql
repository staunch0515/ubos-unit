-- Events of the Server, PH-1 step 15a (DSN-NOD-201, -202, -208, -228; DEC-076).
--
-- * `node_lease`: leadership leases of the nodes (schedules, became-effective, sweeps) with
--   a fencing token that grows with every change of holder (DSN-NOD-228, CTR-008). A node
--   table read and written by the owner role, like the registry.
-- * `event_retention`: the retention in days of each event type of a tenant (`*` for the
--   types without a rule of their own); no rule keeps events (DSN-NOD-202, FR-EVT-015).
-- * indexes of the event log for queries by type, object and time.
-- The outbox's claim columns (`claimed_by`, `claim_until`, `attempts`) exist since 0001.

CREATE TABLE node_lease (
  name         text        PRIMARY KEY,
  holder       text        NOT NULL,
  token        bigint      NOT NULL,
  expires_at   timestamptz NOT NULL,
  acquired_at  timestamptz NOT NULL DEFAULT now());
REVOKE ALL ON node_lease FROM ubos_app;

CREATE TABLE event_retention (
  tenant_id    uuid        NOT NULL,
  type         text        NOT NULL,
  days         integer     NOT NULL CHECK (days > 0),
  PRIMARY KEY (tenant_id, type));
ALTER TABLE event_retention ENABLE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON event_retention USING (tenant_id = nullif(current_setting('ubos.tenant', true), '')::uuid) WITH CHECK (tenant_id = nullif(current_setting('ubos.tenant', true), '')::uuid);
GRANT SELECT, INSERT, UPDATE, DELETE, TRUNCATE ON event_retention TO ubos_app;

CREATE INDEX event_log_type ON event_log (tenant_id, vae_id, type, released_at);
CREATE INDEX event_log_object ON event_log (tenant_id, vae_id, object_id, seq);
CREATE INDEX event_log_time ON event_log (tenant_id, vae_id, released_at);
