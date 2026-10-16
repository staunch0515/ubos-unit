-- The tenant registry and resumable tenant creation, PH-1 step 16b (DSN-NOD-401, -402;
-- FR-TEN-011; DEC-077).
--
-- * `tenant`: the platform settings of a tenant besides its key — plan, placement,
--   residency and the reference of its keys. They are projected into the tenant's VAEs as
--   the System-kind `Tenant` object (read-only to the tenant).
-- * `tenant_setup`: the steps of a tenant's creation already done (register, vae, genesis,
--   projection, invitation, event), so that an interrupted creation resumes where it
--   stopped. Keyed by slug: the first step creates the tenant identifier.

ALTER TABLE tenant
  ADD COLUMN plan      text NOT NULL DEFAULT 'standard',
  ADD COLUMN placement text NOT NULL DEFAULT 'node',
  ADD COLUMN residency text,
  ADD COLUMN keys_ref  text;

CREATE TABLE tenant_setup (
  slug     text        NOT NULL,
  step     text        NOT NULL,
  done_at  timestamptz NOT NULL DEFAULT now(),
  detail   jsonb       NOT NULL DEFAULT '{}'::jsonb,
  PRIMARY KEY (slug, step));
