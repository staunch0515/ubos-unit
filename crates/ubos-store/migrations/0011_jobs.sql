-- Jobs as objects, PH-1 step 15c (DSN-NOD-220…-223, -229, -230; FR-EVT-031…-034, -053;
-- DEC-076). The `job` table is the runtime hint beside the System-kind `Job` objects: the
-- lease, its fencing token and heartbeat, the queue and priority, the poison signature and
-- an operator's cancellation request of a running job; the sweeper rebuilds missing rows
-- from the objects.
--
-- * `job_queue`: per queue of a VAE, paused or not and its concurrency limit.
-- * `job_tenant`: the concurrency limit of a tenant's running jobs (fairness, FR-EVT-034).

ALTER TABLE job ADD COLUMN queue text NOT NULL DEFAULT 'default';
ALTER TABLE job ADD COLUMN priority integer NOT NULL DEFAULT 0;
ALTER TABLE job ADD COLUMN lease_token bigint NOT NULL DEFAULT 0;
ALTER TABLE job ADD COLUMN heartbeat_at timestamptz;
ALTER TABLE job ADD COLUMN error_signature text;
ALTER TABLE job ADD COLUMN signature_count integer NOT NULL DEFAULT 0;
ALTER TABLE job ADD COLUMN poison boolean NOT NULL DEFAULT false;
ALTER TABLE job ADD COLUMN cancel_requested boolean NOT NULL DEFAULT false;
DROP INDEX job_claim;
CREATE INDEX job_claim ON job (tenant_id, vae_id, state, priority DESC, not_before);
CREATE INDEX job_running ON job (tenant_id, state, lease_until);

CREATE TABLE job_queue (
  tenant_id    uuid        NOT NULL,
  vae_id       uuid        NOT NULL,
  queue        text        NOT NULL,
  paused       boolean     NOT NULL DEFAULT false,
  concurrency  integer     CHECK (concurrency > 0),
  PRIMARY KEY (tenant_id, vae_id, queue));

CREATE TABLE job_tenant (
  tenant_id    uuid        PRIMARY KEY,
  concurrency  integer     NOT NULL CHECK (concurrency > 0));

DO $$
DECLARE
  t text;
BEGIN
  FOREACH t IN ARRAY ARRAY['job_queue', 'job_tenant'] LOOP
    EXECUTE format('ALTER TABLE %I ENABLE ROW LEVEL SECURITY', t);
    EXECUTE format('CREATE POLICY tenant_isolation ON %I USING (tenant_id = nullif(current_setting(''ubos.tenant'', true), '''')::uuid) WITH CHECK (tenant_id = nullif(current_setting(''ubos.tenant'', true), '''')::uuid)', t);
    EXECUTE format('GRANT SELECT, INSERT, UPDATE, DELETE, TRUNCATE ON %I TO ubos_app', t);
  END LOOP;
END $$;
