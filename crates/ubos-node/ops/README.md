# SUITE-NOD-OPS (PH-1 part)

`ops_suite.py` installs a single-node Server from its package by the steps of
`../packaging/INSTALL.md` on a PostgreSQL cluster of its own and checks the installation,
the invalid configuration corpus, bootstrap (SCN-001), backups and point-in-time restore at
10 points under load (DSN-NOD-620, -623; SCN-017), the diagnostic bundle (DSN-NOD-615) and
the upgrade with its rollback (DSN-NOD-633): VER-FAULT-2000, EXIT-1-18 (PH-1 step 16f,
DEC-077). Python 3 standard library; the PostgreSQL server binaries must be installed.

```
crates/ubos-node/packaging/build.sh                     # dist/ubos-server-<v>-<arch>-linux.tar.gz
python3 crates/ubos-node/ops/ops_suite.py --package dist/ubos-server-<v>-<arch>-linux.tar.gz \
    --report ops-report.json [--points 10] [--pg-bin /usr/lib/postgresql/16/bin]
```

Run as root, PostgreSQL and the node run as the `postgres` user. The `package` workflow
runs the suite on the x86-64 package; the workstation report is in
`verify/reports/exit-1-18/workstation/`.
