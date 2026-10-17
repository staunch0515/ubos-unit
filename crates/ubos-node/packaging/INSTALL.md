# Installing the UBOS Server (single node, PH-1)

This guide installs a single-node UBOS Server on Linux from the single-binary package
(DSN-NOD-601, FR-OPS-011), bootstraps the first tenant (SCN-001), and sets up backups with
point-in-time restore (DSN-NOD-620, -623) and upgrades (DSN-NOD-633). The suite
SUITE-NOD-OPS (`crates/ubos-node/ops/ops_suite.py`) runs these steps as written. The
container image, the Compose file and the Helm chart are described at the end.

Commands that start with `#` run as root; `postgres$` and `ubos$` mean the PostgreSQL and the
`ubos` service users.

## 1. Requirements

- Linux on x86-64 or ARM64 (the binary is static: no system libraries are needed).
- PostgreSQL 16 or later with the `btree_gist` extension (part of the PostgreSQL
  distribution) and its client tools (`pg_basebackup`), `tar` and `openssl`.
- A directory for backups mounted from another failure domain (another disk, a network
  share): base backups and the WAL archive must survive the loss of the database host.
- PH-1 keeps blobs in PostgreSQL; the S3-compatible object store follows in PH-2.

## 2. Install the binary

```
# sha256sum -c SHA256SUMS --ignore-missing
# tar -xzf ubos-server-<version>-<arch>-linux.tar.gz
# cd ubos-server-<version>-<arch>-linux
# install -m 0755 bin/ubos-server /usr/local/bin/ubos-server
# install -d /usr/local/share/doc/ubos-server && install -m 0644 INSTALL.md ubos.toml.example /usr/local/share/doc/ubos-server/
# ubos-server version
```

Create the service user and its directories; the PostgreSQL user joins the `ubos` group so
that its `archive_command` can read the configuration and write the WAL archive:

```
# useradd --system --home-dir /var/lib/ubos --shell /usr/sbin/nologin ubos
# install -d -o ubos -g ubos -m 0750 /var/lib/ubos
# install -d -o root -g ubos -m 0750 /etc/ubos
# install -d -o ubos -g ubos -m 2770 /var/backups/ubos
# usermod -a -G ubos postgres
```

## 3. Prepare PostgreSQL

The `ubos` role owns the UBOS tables and is not a superuser. The application role
`ubos_app`, which the node switches to for tenant work under row-level security, is created
beforehand and granted to `ubos`. Base backups need `REPLICATION`; restore points need
`pg_create_restore_point`, granted in the `ubos` database (function privileges are per
database):

```
postgres$ psql <<'SQL'
CREATE ROLE ubos_app NOLOGIN;
CREATE ROLE ubos LOGIN REPLICATION PASSWORD '<a long random password>';
GRANT ubos_app TO ubos;
CREATE DATABASE ubos OWNER ubos;
\c ubos
GRANT EXECUTE ON FUNCTION pg_create_restore_point(text) TO ubos;
SQL
```

In `pg_hba.conf`, allow `ubos` to connect to the database and for replication, for example:

```
host  ubos         ubos  127.0.0.1/32  scram-sha-256
host  replication  ubos  127.0.0.1/32  scram-sha-256
```

Turn on continuous WAL archiving in `postgresql.conf` (a restart is needed):

```
wal_level = replica
archive_mode = on
archive_command = '/usr/local/bin/ubos-server backup wal-push %p %f --config /etc/ubos/ubos.toml'
archive_timeout = 60
```

`archive_timeout` bounds how much recent work a restore can lose when the database host is
lost (NR-DUR-002). Restart PostgreSQL after the change.

## 4. Keys and configuration

The root key wraps the tenant keys; the backup key seals the backups and must differ from
it (DSN-NOD-623). Keep a copy of both keys offline: without the backup key no backup can be
read, and without the root key no tenant data can be decrypted.

```
# umask 027
# openssl rand -hex 32 > /etc/ubos/root.key
# openssl rand -hex 32 > /etc/ubos/backup.key
# echo 'host=127.0.0.1 user=ubos dbname=ubos password=<the password>' > /etc/ubos/database.url
# cp /usr/local/share/doc/ubos-server/ubos.toml.example /etc/ubos/ubos.toml
# chgrp ubos /etc/ubos/*.key /etc/ubos/database.url /etc/ubos/ubos.toml
# chmod 0640 /etc/ubos/*.key /etc/ubos/database.url /etc/ubos/ubos.toml
```

Edit `/etc/ubos/ubos.toml` (tenant, VAE, listen address; the example's comments name each
key; `ubos-server config-schema` lists them all) and check it. An invalid file is rejected
with every error named, and nothing is applied:

```
# ubos-server check-config /etc/ubos/ubos.toml
```

## 5. Bootstrap

`bootstrap` checks the prerequisites (PostgreSQL version, extensions, clock, root key),
applies the migrations, creates the tenant and its VAE, installs the signed genesis Buks
`system` and `ontology` and prints a one-time administrator invitation, valid for 24 hours.
It can run again: an interrupted bootstrap completes where it stopped.

```
ubos$ ubos-server bootstrap --config /etc/ubos/ubos.toml
```

Set the administrator's password through the link it prints:

```
$ curl -X POST <link> -H 'content-type: application/json' -d '{"password": "…"}'
```

`ubos-server bootstrap --new-invitation` prints a new link when the first one expired.

## 6. Start the service and the backups

```
# cp systemd/ubos-server.service systemd/ubos-backup.service systemd/ubos-backup.timer /etc/systemd/system/
# systemctl daemon-reload
# systemctl enable --now ubos-server ubos-backup.timer
ubos$ ubos-server backup base --config /etc/ubos/ubos.toml
```

The first base backup starts the point-in-time window; the timer takes one every day and
removes what `backup.retention_days` no longer needs. The node creates a named restore
point every `backup.restore_point_minutes`, recording the branch heads of every VAE.

## 7. Check the installation

```
$ curl -s http://127.0.0.1:8080/readyz
$ curl -s -H "authorization: Bearer <token>" http://127.0.0.1:8080/api/v1/admin/health
ubos$ ubos-server backup list --config /etc/ubos/ubos.toml
```

`/readyz` answers `ready` with the database and key service checked; `health` lists the
self-checks (clock, certificate, keys, disk, backup age, WAL archive, DVM verifier,
isolation, telemetry). The token comes from `POST /api/v1/identity/sign-in`.

## 8. Operations

**Configuration.** Edit `/etc/ubos/ubos.toml`, check it, then `systemctl reload ubos-server`
(SIGHUP). Reloadable keys apply at once; the others report "restart required". Every
applied version is recorded with its hash, author and diff
(`GET /api/v1/admin/config/versions`).

**Diagnostics.** `POST /api/v1/admin/diagnostics` returns a bundle of versions, configuration
(references only), health, metrics, recent spans and logs and database metadata, without
business data. When the node does not start: `ubos-server diagnostics --config
/etc/ubos/ubos.toml --out bundle.json`.

**Restore to a point.** Restore into a new data directory and a new port, verify, then move
the service over. As the PostgreSQL user:

```
postgres$ ubos-server backup list --config /etc/ubos/ubos.toml
postgres$ ubos-server restore --config /etc/ubos/ubos.toml --into /var/lib/postgresql/restore \
            --point <name> --port 5433 --hba-from /etc/postgresql/16/main/pg_hba.conf
postgres$ pg_ctl -D /var/lib/postgresql/restore -w start
ubos$ ubos-server verify --config /etc/ubos/ubos.toml --point <name> \
        --database 'host=127.0.0.1 port=5433 user=ubos dbname=ubos password=…'
```

`--time 2026-10-17T09:30:00Z` restores to a time, `--latest` to the end of the archive.
`verify` compares the restored branch heads with those recorded at the point and runs the
DVM root verifier and the index rebuild check over the last 1,000 commits of every VAE; its
report is written under `/var/backups/ubos/reports/`. When the WAL archive has a gap, the
restore refuses and names the last consistent point.

**Upgrade.** One command checks the new binary and the configuration, takes a base backup
and a restore point, stops the service, applies the migrations with the new binary,
installs it (the old one is kept as `ubos-server.previous`), starts the service and
verifies against the point:

```
# ubos-server upgrade --config /etc/ubos/ubos.toml --binary ./ubos-server-<new>/bin/ubos-server --service ubos-server
```

The report is written under `/var/backups/ubos/reports/`. When a step after the
installation fails, the old binary is put back; the database is restored to the point if
the new migrations must be undone.

## 9. Containers and Kubernetes

**OCI image.** From the repository root:
`docker build -f crates/ubos-node/packaging/Dockerfile -t ubos-server .` The image is based on
PostgreSQL 16's, so it carries `pg_basebackup` and `tar`, and the same image can run the
database with the UBOS `archive_command`.

**Docker Compose.** `crates/ubos-node/packaging/compose.yaml` runs PostgreSQL with WAL
archiving, the bootstrap, the Server and a daily base backup. Its header lists the secrets
to create. The invitation link is in `docker compose logs bootstrap`.

**Helm.** `crates/ubos-node/packaging/helm/ubos-server` deploys one replica against an
existing PostgreSQL prepared as in section 3, with a Secret `ubos-keys` holding `root.key`,
`backup.key` and `database.url`, a volume for backups and a daily backup CronJob. The WAL
archiving runs on the PostgreSQL side, which needs the `ubos-server` binary and the backup
volume.

## 10. Troubleshooting

| Symptom | Cause and fix |
|---|---|
| `bootstrap` stops at `extensions` | `btree_gist` is missing: install the PostgreSQL contrib package |
| `permission denied to set role "ubos_app"` | `GRANT ubos_app TO ubos;` (section 3) |
| self-check `wal archive` is `error` | PostgreSQL cannot run `archive_command`: check that `postgres` is in the `ubos` group and see the PostgreSQL log |
| `backup point` fails with `permission denied for function pg_create_restore_point` | in the `ubos` database: `GRANT EXECUTE ON FUNCTION pg_create_restore_point(text) TO ubos;` |
| `restore` refuses with `wal_chain_broken` | a WAL segment is missing from the archive: restore to the named `last_consistent` point |
| `/readyz` reports `degraded` with `read_only` | the database refuses writes (read-only or in recovery); reads keep working and writes return when it recovers |
