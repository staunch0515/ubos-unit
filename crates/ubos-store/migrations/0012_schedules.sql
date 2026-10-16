-- Schedules, PH-1 step 15d (DSN-NOD-224…-226; FR-EVT-041…-044; DEC-076).
--
-- * `schedule_cursor`: per schedule of a VAE, the instant up to which its slots have been
--   evaluated (the start of the next evaluation, and of the missed slots after an outage).
-- * `schedule_slot`: one row per slot of a schedule, unique by `(schedule, slot)`: `fired`
--   (its job enqueued), `missed` and `skipped` (by the misfire policy or the window),
--   `paused` (recorded while paused, decided on resume), `caught_up` (fired on resume).
--   The unique key makes a slot fire at most once whatever the number of leaders.

CREATE TABLE schedule_cursor (
  tenant_id    uuid        NOT NULL,
  vae_id       uuid        NOT NULL,
  schedule_id  text        NOT NULL,
  cursor_at    timestamptz NOT NULL,
  PRIMARY KEY (tenant_id, vae_id, schedule_id));

CREATE TABLE schedule_slot (
  tenant_id    uuid        NOT NULL,
  vae_id       uuid        NOT NULL,
  schedule_id  text        NOT NULL,
  slot         timestamptz NOT NULL,
  status       text        NOT NULL,
  job_id       uuid,
  recorded_at  timestamptz NOT NULL DEFAULT now(),
  PRIMARY KEY (tenant_id, vae_id, schedule_id, slot));

DO $$
DECLARE
  t text;
BEGIN
  FOREACH t IN ARRAY ARRAY['schedule_cursor', 'schedule_slot'] LOOP
    EXECUTE format('ALTER TABLE %I ENABLE ROW LEVEL SECURITY', t);
    EXECUTE format('CREATE POLICY tenant_isolation ON %I USING (tenant_id = nullif(current_setting(''ubos.tenant'', true), '''')::uuid) WITH CHECK (tenant_id = nullif(current_setting(''ubos.tenant'', true), '''')::uuid)', t);
    EXECUTE format('GRANT SELECT, INSERT, UPDATE, DELETE, TRUNCATE ON %I TO ubos_app', t);
  END LOOP;
END $$;
