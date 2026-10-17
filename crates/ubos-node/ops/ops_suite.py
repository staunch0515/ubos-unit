#!/usr/bin/env python3
"""SUITE-NOD-OPS, PH-1 part (PH-1 step 16f, DEC-077; VER-FAULT-2000, EXIT-1-18).

Installs a single-node Server from its package by the steps of INSTALL.md on a PostgreSQL
cluster of its own, then checks:

  install    the package's checksum, the binary, the configuration (DSN-NOD-601, FR-OPS-011)
  configs    the invalid configuration corpus, every file rejected (NR-OPER-003, >= 200)
  bootstrap  SCN-001: an interrupted bootstrap completes, the genesis Buks recorded
  backups    a base backup, then restore points under a continuous commit load
  bundle     a diagnostic bundle within 5 minutes without business values (NR-OPER-005)
  pitr       every point restored into a new cluster: heads equal, DVM roots and indexes
             verified (DSN-NOD-620 acceptance 1); a broken WAL chain refused (SCN-017 A1)
  upgrade    one command with backup, migration, verification and report; a failed
             verification puts the previous binary back (DSN-NOD-633)

Usage:
  ops_suite.py --package dist/ubos-server-<v>-<arch>-linux.tar.gz [--points 10]
               [--work DIR] [--report FILE] [--pg-bin DIR]
  ops_suite.py --binary target/release/ubos-server ...   (without a package)

Runs as an ordinary user with the PostgreSQL server binaries installed; as root, PostgreSQL
runs as the `postgres` user. Standard library only.
"""

import argparse
import glob
import hashlib
import json
import os
import platform
import re
import secrets
import shutil
import signal
import socket
import subprocess
import sys
import tarfile
import threading
import time
import urllib.error
import urllib.request

ROOT = os.geteuid() == 0
CHECKS = []
STEPS = {}


def check(name, ok, detail=""):
    CHECKS.append({"check": name, "ok": bool(ok), "detail": None if ok else str(detail)[:2000]})
    print(("  ok   " if ok else "  FAIL ") + name + ("" if ok else "  " + str(detail)[:600]), flush=True)
    return ok


def free_port():
    s = socket.socket()
    s.bind(("127.0.0.1", 0))
    p = s.getsockname()[1]
    s.close()
    return p


class Env:
    def __init__(self, a):
        self.a = a
        self.work = os.path.abspath(a.work)
        self.pg_bin = a.pg_bin or self._pg_bin()
        self.pg_user = "postgres" if ROOT else None
        self.pg_port = free_port()
        self.http_port = free_port()
        self.url = f"http://127.0.0.1:{self.http_port}"
        self.sock = os.path.join(self.work, "sock")
        self.db_password = secrets.token_hex(16)
        self.canary = "canary" + secrets.token_hex(6)

    @staticmethod
    def _pg_bin():
        try:
            return subprocess.run(["pg_config", "--bindir"], capture_output=True, text=True, check=True).stdout.strip()
        except Exception:
            found = sorted(glob.glob("/usr/lib/postgresql/*/bin"))
            if not found:
                sys.exit("no PostgreSQL server binaries: pass --pg-bin")
            return found[-1]

    def run(self, *args, pg=False, env=None, timeout=900):
        args = list(args)
        # as root, the node and its commands run as the PostgreSQL user, who owns the work
        # directory (the guide's `ubos` user and the `postgres` user in one)
        pg = pg or (getattr(self, "bin", None) is not None and args[0] == self.bin and getattr(self, "owned", False))
        if pg and self.pg_user:
            args = ["runuser", "-u", self.pg_user, "--"] + args
        e = dict(os.environ)
        e.update(env or {})
        return subprocess.run(args, capture_output=True, text=True, env=e, timeout=timeout)

    def psql(self, sql, port=None, db="postgres"):
        """Each statement of `sql` (a text or a list) in its own transaction."""
        stmts = sql if isinstance(sql, list) else [sql]
        cmd = [os.path.join(self.pg_bin, "psql"), "-h", self.sock, "-p", str(port or self.pg_port),
               "-U", "postgres", "-d", db, "-v", "ON_ERROR_STOP=1", "-At"]
        for s in stmts:
            cmd += ["-c", s]
        return self.run(*cmd, pg=True)

    def ubos_url(self, port=None):
        return f"host=127.0.0.1 port={port or self.pg_port} user=ubos dbname=ubos password={self.db_password}"

    def http(self, method, path, body=None, token=None, timeout=300):
        req = urllib.request.Request(self.url + path, method=method,
                                     data=None if body is None else json.dumps(body).encode())
        req.add_header("content-type", "application/json")
        if token:
            req.add_header("authorization", "Bearer " + token)
        try:
            with urllib.request.urlopen(req, timeout=timeout) as r:
                return r.status, json.loads(r.read() or b"null")
        except urllib.error.HTTPError as e:
            t = e.read()
            try:
                return e.code, json.loads(t)
            except Exception:
                return e.code, t.decode(errors="replace")
        except Exception as e:
            return 0, str(e)


# ---- install (INSTALL.md sections 2 and 4, under a private prefix) ----

def install(env):
    t0 = time.time()
    a = env.a
    prefix = os.path.join(env.work, "prefix")
    os.makedirs(os.path.join(prefix, "bin"))
    if a.package:
        sums = os.path.join(os.path.dirname(os.path.abspath(a.package)), "SHA256SUMS")
        digest = hashlib.sha256(open(a.package, "rb").read()).hexdigest()
        listed = open(sums).read() if os.path.exists(sums) else ""
        check("install: the package matches SHA256SUMS", f"{digest}  {os.path.basename(a.package)}" in listed, listed[:300])
        with tarfile.open(a.package) as t:
            t.extractall(env.work, filter="data")
        top = os.path.join(env.work, os.path.basename(a.package)[: -len(".tar.gz")])
        shutil.copy(os.path.join(top, "bin", "ubos-server"), os.path.join(prefix, "bin", "ubos-server"))
        example = os.path.join(top, "ubos.toml.example")
        check("install: the package carries the guide and the systemd units",
              os.path.exists(os.path.join(top, "INSTALL.md")) and len(glob.glob(os.path.join(top, "systemd", "*"))) == 3)
    else:
        shutil.copy(a.binary, os.path.join(prefix, "bin", "ubos-server"))
        example = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "packaging", "ubos.toml.example")
    os.chmod(os.path.join(prefix, "bin", "ubos-server"), 0o755)
    env.bin = os.path.join(prefix, "bin", "ubos-server")
    v = env.run(env.bin, "version")
    check("install: the binary runs", v.returncode == 0 and v.stdout.startswith("ubos-server "), v.stderr)
    env.version = v.stdout.strip()
    # keys and configuration
    etc = os.path.join(env.work, "etc")
    os.makedirs(etc)
    env.backups = os.path.join(env.work, "backups")
    os.makedirs(env.backups)
    os.chmod(env.backups, 0o2770)
    for k in ["root.key", "backup.key"]:
        open(os.path.join(etc, k), "w").write(secrets.token_hex(32) + "\n")
    open(os.path.join(etc, "database.url"), "w").write(env.ubos_url() + "\n")
    text = open(example).read()
    subst = {
        'listen = "0.0.0.0:8080"': f'listen = "127.0.0.1:{env.http_port}"',
        "/etc/ubos/": etc + "/",
        "/var/backups/ubos": env.backups,
        "restore_point_minutes = 10": "restore_point_minutes = 0",
    }
    for k, v in subst.items():
        text = text.replace(k, v)
    env.cfg = os.path.join(etc, "ubos.toml")
    open(env.cfg, "w").write(text)
    r = env.run(env.bin, "check-config", env.cfg)
    check("install: the configuration from the example is valid", r.returncode == 0, r.stdout + r.stderr)
    STEPS["install_seconds"] = round(time.time() - t0, 2)


# ---- PostgreSQL (INSTALL.md section 3) ----

def postgres(env):
    t0 = time.time()
    data = os.path.join(env.work, "pgdata")
    os.makedirs(env.sock)
    if ROOT:
        subprocess.run(["chown", "-R", "postgres", env.work])
        os.chmod(env.work, 0o755)
        env.owned = True
    r = env.run(os.path.join(env.pg_bin, "initdb"), "-D", data, "-U", "postgres", "--auth-local=trust",
                "--auth-host=scram-sha-256", "-E", "UTF8", pg=True)
    if not check("postgres: initdb", r.returncode == 0, r.stderr):
        return False
    conf = (f"\nport = {env.pg_port}\nlisten_addresses = '127.0.0.1'\nunix_socket_directories = '{env.sock}'\n"
            f"wal_level = replica\narchive_mode = on\n"
            f"archive_command = '{env.bin} backup wal-push %p %f --config {env.cfg}'\narchive_timeout = 60\n"
            f"max_connections = 200\n")
    with open(os.path.join(data, "postgresql.conf"), "a") as f:
        f.write(conf)
    with open(os.path.join(data, "pg_hba.conf"), "a") as f:
        f.write("host  ubos         ubos  127.0.0.1/32  scram-sha-256\nhost  replication  ubos  127.0.0.1/32  scram-sha-256\n")
    env.pg_data = data
    r = env.run(os.path.join(env.pg_bin, "pg_ctl"), "-D", data, "-l", data + ".log", "-w", "start", pg=True)
    if not check("postgres: started with WAL archiving", r.returncode == 0, r.stderr):
        return False
    r = env.psql([
        "CREATE ROLE ubos_app NOLOGIN",
        f"CREATE ROLE ubos LOGIN REPLICATION PASSWORD '{env.db_password}'",
        "GRANT ubos_app TO ubos",
        "CREATE DATABASE ubos OWNER ubos"])
    if r.returncode == 0:
        # function privileges are per database
        r = env.psql("GRANT EXECUTE ON FUNCTION pg_create_restore_point(text) TO ubos", db="ubos")
    check("postgres: the ubos role and database (not a superuser)", r.returncode == 0, r.stderr)
    STEPS["postgres_seconds"] = round(time.time() - t0, 2)
    return True


# ---- the invalid configuration corpus (NR-OPER-003, VER-FAULT-2000 step 2) ----

def corpus():
    base = {"server": {"listen": "127.0.0.1:8080", "preset": "server", "tenant": "acme", "vae": "acme"},
            "database": {"url": "env:UBOS_DATABASE_URL"}, "keys": {"root_kek": "env:UBOS_ROOT_KEK"},
            "log": {"level": "info"}, "limits": {"requests_per_second": 10, "burst": 20}}

    def toml(d, version=1):
        lines = [] if version is None else [f"version = {version if isinstance(version, str) else json.dumps(version)}"]
        for sec, kv in d.items():
            lines.append(f"[{sec}]")
            lines += [f"{k} = {json.dumps(v)}" for k, v in kv.items()]
        return "\n".join(lines) + "\n"

    def mut(sec, k, v):
        d = json.loads(json.dumps(base))
        d.setdefault(sec, {})[k] = v
        return toml(d)

    keys = {
        "server.listen": ["", "localhost", "8080", "127.0.0.1", "127.0.0.1:99999", "a b:1", 1, True, ["x"], "http://x:1"],
        "server.preset": ["prod", "", "DEV", 1, False, "dev ", "server\n", ["dev"], "edge", "box"],
        "server.tenant": ["", "Acme", "acme_1", "-acme", "a" * 64, "ac me", 1, "acme!", "ACME", "é"],
        "server.vae": ["", "Vae", "x_y", "-x", "b" * 64, 2, "a.b", "a/b", "A", "sp ace"],
        "database.url": ["postgres://u:secret@h/db", "host=h user=u password=x", "postgresql://admin:pw@db:5432/x",
                         "PASSWORD=abc host=h", "", 5, True, "env:lower", "file:relative", "kms:/nokey"],
        "keys.root_kek": ["00" * 32, "plain", "env:", "file:", "kms:aws", "kms:AWS/key", "env:1BAD", "file:/a b", 3, "secret"],
        "log.level": ["verbose", "", "INFO", 3, "warning", "fatal", "inf", True, "debug ", "off"],
        "telemetry.export": ["otel", "", "jaeger", 1, "OTLP", "stdout", "file", False, "none ", "zipkin"],
        "telemetry.otlp_endpoint": ["ftp://x", "localhost:4318", "http://", "", 1, "https:// x", "grpc://h:4317",
                                    "http//h", "/v1/traces", True],
        "telemetry.sample_ratio": [1.5, -0.1, "0.5", 2, -1, True, "all", 10, 1.0001, -0.0001],
        "limits.requests_per_second": [-1, 100001, "10", 1.5, True, "", 1000000, -100, "none", [1]],
        "limits.burst": [0, -5, 100001, "20", 2.5, False, "", 1e9, "max", [2]],
        "backup.dir": ["relative/path", "", 1, "~/backups", "./b", True, "c:\\b", "b/", "x", ["/b"]],
        "backup.key": ["00" * 32, "key", "env:", "file:x", "kms:", "kms:gcp", "env:9X", "raw:abc", 1, True],
        "backup.retention_days": [0, 366, -1, "35", 1.5, True, 1000, "", -35, [35]],
        "backup.restore_point_minutes": [-1, 1441, "10", 2.5, True, "", 100000, -10, "often", [10]],
        "bootstrap.admin_username": ["", 1, True, "x" * 1025, ["a"], 2.5, 7, False, 0, -1],
    }
    cases = []
    for path, vals in keys.items():
        sec, k = path.split(".")
        cases += [(f"{path}={v!r}"[:70], mut(sec, k, v)) for v in vals]
    for sec, k in [("server", "port"), ("database", "password"), ("log", "format"), ("limits", "rps"), ("nope", "x"),
                   ("telemetry", "endpoint"), ("backup", "path"), ("keys", "tenant_kek"), ("server", "listen_addr"),
                   ("bootstrap", "password")]:
        cases.append((f"unknown {sec}.{k}", mut(sec, k, "x")))
    for sec in ["server", "database", "keys", "log", "telemetry", "limits", "backup", "bootstrap"]:
        cases.append((f"{sec} not a table", f"version = 1\n{sec} = 1\n"))
    for path in ["server.listen", "log.level", "limits.burst", "backup.retention_days"]:
        sec, k = path.split(".")
        cases.append((f"{path} a table", f"version = 1\n[{sec}]\n{k} = {{ x = 1 }}\n"))
    cases.append(("no version", toml(base, None)))
    for v in [0, 2, '"1"', '"one"', 1.0, -1, 99]:
        cases.append((f"version {v}", toml(base, v)))
    for name, txt in [("not TOML", "version = 1\n[server\nlisten=1"), ("bare key", 'version = 1\nlisten = "x"'),
                      ("duplicate key", 'version = 1\n[log]\nlevel="info"\nlevel="debug"'),
                      ("array section", "version = 1\nserver = [1,2]"),
                      ("unterminated string", 'version = 1\n[log]\nlevel = "info'), ("empty", ""), ("garbage", "%%%"),
                      ("inline table value", "version = 1\n[server]\nlisten = { a = 1 }"),
                      ("duplicate section", 'version=1\n[log]\nlevel="info"\n[log]\nlevel="debug"'),
                      ("bad escape", 'version=1\n[log]\nlevel="\\q"')]:
        cases.append((name, txt))
    for name, extra in [("otlp without endpoint", {"telemetry": {"export": "otlp"}}),
                        ("backup dir without key", {"backup": {"dir": "/b"}}),
                        ("backup key without dir", {"backup": {"key": "env:K"}})]:
        d = json.loads(json.dumps(base))
        d.update(extra)
        cases.append((name, toml(d)))
    d = json.loads(json.dumps(base))
    del d["keys"]
    cases.append(("server with a database without a root key", toml(d)))
    return cases


def configs(env):
    t0 = time.time()
    d = os.path.join(env.work, "invalid")
    os.makedirs(d)
    accepted = []
    cases = corpus()
    for i, (name, text) in enumerate(cases):
        p = os.path.join(d, "%03d.toml" % i)
        open(p, "w").write(text)
        r = env.run(env.bin, "check-config", p)
        if r.returncode != 2 or "invalid" not in r.stderr:
            accepted.append(name)
    check(f"configs: {len(cases)} invalid configurations, every one rejected (NR-OPER-003)",
          len(cases) >= 200 and not accepted, accepted)
    STEPS["invalid_configurations"] = len(cases)
    STEPS["configs_seconds"] = round(time.time() - t0, 2)


# ---- bootstrap (SCN-001) and the node ----

def bootstrap(env):
    t0 = time.time()
    r = env.run(env.bin, "bootstrap", "--config", env.cfg, env={"UBOS_FAULT_AFTER_STEP": "genesis"})
    check("bootstrap: interrupted after the genesis step (SCN-001 A1)", r.returncode == 86, r.stdout + r.stderr)
    r = env.run(env.bin, "bootstrap", "--config", env.cfg)
    m = re.search(r"(http://\S+/api/v1/identity/invitations/\S+)", r.stdout)
    check("bootstrap: the rerun completes and prints the invitation", r.returncode == 0 and m, r.stdout + r.stderr)
    env.invitation = m.group(1) if m else None
    STEPS["bootstrap_seconds"] = round(time.time() - t0, 2)


def start_node(env):
    log = open(os.path.join(env.work, "server.log"), "a")
    cmd = [env.bin, "--config", env.cfg]
    if ROOT:
        cmd = ["runuser", "-u", "postgres", "--"] + cmd
    p = subprocess.Popen(cmd, stdout=log, stderr=subprocess.STDOUT)
    for _ in range(300):
        st, r = env.http("GET", "/readyz", timeout=5)
        if st == 200:
            break
        time.sleep(0.2)
    return p


def stop_node(p):
    p.send_signal(signal.SIGTERM)
    try:
        p.wait(60)
    except subprocess.TimeoutExpired:
        p.kill()


def sign_in(env):
    path = env.invitation.split(env.url)[1] if env.invitation and env.url in env.invitation else None
    if path:
        env.http("POST", path, {"password": "correct horse battery staple"})
    st, r = env.http("POST", "/api/v1/identity/sign-in", {"username": "admin", "password": "correct horse battery staple"})
    check("bootstrap: the administrator signs in through the invitation", st == 200 and "token" in r, r)
    env.token = r.get("token") if isinstance(r, dict) else None


def genesis(env):
    st, r = env.http("GET", "/api/v1/acme/Buk", token=env.token)
    names = sorted(x.get("body", {}).get("name", "") for x in (r.get("items", []) if isinstance(r, dict) else []))
    check("bootstrap: the genesis Buks system and ontology recorded once each (SCN-001)",
          st == 200 and names == ["ontology", "system"], r)


class Load(threading.Thread):
    """Commits of Person objects carrying the canary value, one after another."""

    def __init__(self, env):
        super().__init__(daemon=True)
        self.env, self.n, self.errors, self.stop = env, 0, 0, threading.Event()

    def commit(self, k=3):
        e = self.env
        changes = []
        for _ in range(k):
            self.n += 1
            changes.append({"op": "create", "class": "Person", "object_id": "01890a5d-ac96-7001-8000-%012x" % (0x10000 + self.n),
                            "slug": f"load{self.n}", "body": {"name": f"{e.canary} {self.n}", "email": f"{e.canary}{self.n}@example.com"}})
        msgs = [{"v": "1.0", "id": "h", "type": "handshake",
                 "body": {"vae": "acme", "versions": ["1.0"], "credentials": {"kind": "bearer", "token": e.token}}},
                {"v": "1.0", "id": "m", "type": "mutate", "body": {"vae": "acme", "branch": "main", "commit": {"changes": changes}}}]
        st, r = e.http("POST", "/ubtp/v1", msgs)
        ok = st == 200 and r["responses"][1]["type"] == "result"
        self.errors += 0 if ok else 1
        return ok

    def run(self):
        while not self.stop.is_set():
            self.commit()
            time.sleep(0.05)


# ---- backups, the bundle, restores ----

def backups(env, load):
    t0 = time.time()
    r = env.run(env.bin, "backup", "base", "--config", env.cfg, timeout=1800)
    check("backups: a base backup under load", r.returncode == 0, r.stdout[-500:] + r.stderr[-500:])
    env.points = []
    for i in range(env.a.points):
        time.sleep(env.a.spacing)
        r = env.run(env.bin, "backup", "point", f"p{i:02d}", "--config", env.cfg)
        if r.returncode == 0:
            env.points.append(json.loads(r.stdout)["name"])
        env.psql("SELECT pg_switch_wal()")
    check(f"backups: {env.a.points} restore points under load with recorded heads", len(env.points) == env.a.points, env.points)
    last = json.loads(env.run(env.bin, "backup", "point", "final", "--config", env.cfg).stdout or "{}").get("wal_file", "")
    env.psql("SELECT pg_switch_wal()")
    archived = ""
    for _ in range(120):
        archived = env.psql("SELECT coalesce(last_archived_wal, '') FROM pg_stat_archiver").stdout.strip()
        if archived and last and archived[8:] >= last[8:]:
            break
        time.sleep(1)
    check("backups: the WAL archived through the last point", archived and last and archived[8:] >= last[8:], (archived, last))
    files = glob.glob(os.path.join(env.backups, "**", "*.enc"), recursive=True)
    blob = b"".join(open(f, "rb").read() for f in files)
    check("backups: no business value in the backup files (DSN-NOD-623)", env.canary.encode() not in blob and len(files) > 3, len(files))
    other = env.cfg + ".otherkey"
    okey = os.path.join(os.path.dirname(env.cfg), "other.key")
    open(okey, "w").write(secrets.token_hex(32))
    open(other, "w").write(open(env.cfg).read().replace("backup.key", "other.key"))
    r = env.run(env.bin, "backup", "list", "--config", other)
    check("backups: unreadable with another key (DSN-NOD-623 acceptance 1)", r.returncode != 0 and "another backup key" in r.stderr, r.stderr)
    STEPS["backup_seconds"] = round(time.time() - t0, 2)
    STEPS["commits_under_load"] = load.n


def bundle(env):
    t0 = time.time()
    st, b = env.http("POST", "/api/v1/admin/diagnostics", {}, token=env.token, timeout=300)
    dt = time.time() - t0
    text = json.dumps(b)
    check("bundle: produced within 5 minutes (NR-OPER-005)", st == 200 and dt <= 300, (st, dt))
    check("bundle: no business value, no secret (DSN-NOD-615)",
          env.canary not in text and env.db_password not in text, "canary or password found")
    STEPS["bundle_seconds"] = round(dt, 2)


def restore(env, args, port, cfg=None, hba=True):
    into = os.path.join(env.work, f"restore-{port}")
    extra = ["--hba-from", os.path.join(env.pg_data, "pg_hba.conf")] if hba else []
    r = env.run(env.bin, "restore", "--config", cfg or env.cfg, "--into", into, "--port", str(port), *extra, *args,
                pg=True, timeout=1800)
    return into, r


def start_restored(env, into, port):
    r = env.run(os.path.join(env.pg_bin, "pg_ctl"), "-D", into, "-l", into + ".log", "-o", f"-k {env.sock}", "-w",
                "-t", "300", "start", pg=True)
    if r.returncode != 0:
        return (r.stderr + open(into + ".log").read()[-1000:]) if os.path.exists(into + ".log") else r.stderr
    for _ in range(600):
        if env.psql("SELECT pg_is_in_recovery()", port=port).stdout.strip() == "f":
            return None
        time.sleep(0.5)
    return "still in recovery"


def stop_restored(env, into):
    env.run(os.path.join(env.pg_bin, "pg_ctl"), "-D", into, "-m", "fast", "-w", "stop", pg=True)
    shutil.rmtree(into, ignore_errors=True)
    shutil.rmtree(into + ".wal", ignore_errors=True)


def pitr(env):
    t0 = time.time()
    times = []
    verified = 0
    for i, p in enumerate(env.points):
        port = free_port()
        s = time.time()
        into, r = restore(env, ["--point", p], port)
        err = r.stderr + r.stdout if r.returncode != 0 else start_restored(env, into, port)
        if err:
            check(f"pitr: restore to {p}", False, err)
            stop_restored(env, into)
            continue
        v = env.run(env.bin, "verify", "--config", env.cfg, "--database", env.ubos_url(port), "--point", p, "--last", "1000",
                    timeout=1800)
        vj = json.loads(v.stdout or "{}") if v.stdout.strip().startswith("{") else {}
        ok = v.returncode == 0 and vj.get("ok") is True and vj.get("heads_equal") is True
        check(f"pitr: {p} restored, heads equal, DVM roots and indexes of the last 1,000 commits verified", ok,
              v.stdout[-800:] + v.stderr[-400:])
        verified += ok
        times.append(round(time.time() - s, 2))
        stop_restored(env, into)
    STEPS["restores_verified"] = verified
    STEPS["restore_seconds"] = times
    # a broken chain (SCN-017 A1)
    broken = os.path.join(env.work, "backups-broken")
    shutil.copytree(env.backups, broken)
    cfg = env.cfg + ".broken"
    open(cfg, "w").write(open(env.cfg).read().replace(env.backups, broken))
    listing = json.loads(env.run(env.bin, "backup", "list", "--config", env.cfg).stdout)
    start = listing["base_backups"][0]["start_wal"]
    mid = [x for x in listing["points"] if x["name"] == env.points[len(env.points) // 2]][0]["wal_file"]
    segs = sorted(f[:24] for f in os.listdir(os.path.join(broken, "wal")) if len(f) == 28)
    gap = [s for s in segs if start[8:] < s[8:] < mid[8:]]
    if ROOT:
        subprocess.run(["chown", "-R", "postgres", broken])
    if gap:
        os.remove(os.path.join(broken, "wal", gap[-1] + ".enc"))
        into, r = restore(env, ["--point", env.points[-1]], free_port(), cfg=cfg)
        d = json.loads(r.stdout) if r.stdout.strip().startswith("{") else {}
        check("pitr: a restore past a gap in the WAL chain refused with the last consistent point (SCN-017 A1)",
              r.returncode != 0 and d.get("reason") == "wal_chain_broken" and d.get("last_consistent"), r.stdout + r.stderr)
        shutil.rmtree(into, ignore_errors=True)
    else:
        check("pitr: a WAL segment between the base backup and the middle point to remove", False, segs)
    STEPS["pitr_seconds"] = round(time.time() - t0, 2)


# ---- upgrade (DSN-NOD-633) ----

def upgrade(env):
    t0 = time.time()
    new = os.path.join(env.work, "new")
    os.makedirs(new)
    shutil.copy(env.bin, os.path.join(new, "ubos-server"))
    r = env.run(env.bin, "upgrade", "--config", env.cfg, "--binary", os.path.join(new, "ubos-server"), timeout=1800)
    j = json.loads(r.stdout) if r.stdout.strip().startswith("{") else {}
    done = {s["step"] for s in j.get("steps", []) if s.get("ok")}
    check("upgrade: one command — configuration, backup, point, migrations, install, verification, report",
          r.returncode == 0 and j.get("ok") and {"base backup", "restore point", "migrations", "install", "verification"} <= done
          and j.get("report") and os.path.exists(j["report"]), r.stdout[-1200:] + r.stderr[-400:])
    STEPS["upgrade_seconds"] = round(time.time() - t0, 2)
    # a new binary whose verification fails: the previous binary comes back
    bad = os.path.join(env.work, "bad")
    os.makedirs(bad)
    real = os.path.join(bad, "ubos-server.real")
    shutil.copy(env.bin, real)
    wrapper = os.path.join(bad, "ubos-server")
    open(wrapper, "w").write(f'#!/bin/sh\nif [ "$1" = verify ]; then echo "verification broken" >&2; exit 1; fi\nexec {real} "$@"\n')
    os.chmod(wrapper, 0o755)
    before = hashlib.sha256(open(env.bin, "rb").read()).hexdigest()
    r = env.run(env.bin, "upgrade", "--config", env.cfg, "--binary", wrapper, timeout=1800)
    j = json.loads(r.stdout) if r.stdout.strip().startswith("{") else {}
    after = hashlib.sha256(open(env.bin, "rb").read()).hexdigest()
    check("upgrade: a failed verification puts the previous binary back",
          r.returncode != 0 and any(s["step"] == "rollback" and s["ok"] for s in j.get("steps", [])) and before == after,
          r.stdout[-1200:])


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    src = ap.add_mutually_exclusive_group(required=True)
    src.add_argument("--package")
    src.add_argument("--binary")
    ap.add_argument("--work", default=None)
    ap.add_argument("--report", default=None)
    ap.add_argument("--points", type=int, default=10)
    ap.add_argument("--spacing", type=float, default=2.0, help="seconds of load between restore points")
    ap.add_argument("--pg-bin", default=None)
    ap.add_argument("--keep", action="store_true", help="keep the work directory")
    a = ap.parse_args()
    a.work = a.work or (f"/var/tmp/ubos-ops-{os.getpid()}" if ROOT else os.path.join(os.getcwd(), f"ubos-ops-{os.getpid()}"))
    if os.path.exists(a.work):
        sys.exit(f"{a.work} exists")
    os.makedirs(a.work)
    env = Env(a)
    started = time.time()
    node = None
    load = None
    try:
        print("== install (DSN-NOD-601)", flush=True)
        install(env)
        print("== PostgreSQL (INSTALL.md section 3)", flush=True)
        if not postgres(env):
            raise SystemExit
        print("== invalid configurations (NR-OPER-003)", flush=True)
        configs(env)
        print("== bootstrap (SCN-001)", flush=True)
        bootstrap(env)
        node = start_node(env)
        sign_in(env)
        genesis(env)
        print("== backups under load (DSN-NOD-620, -623)", flush=True)
        load = Load(env)
        check("load: commits", load.commit())
        load.start()
        backups(env, load)
        print("== diagnostic bundle (DSN-NOD-615)", flush=True)
        bundle(env)
        load.stop.set()
        load.join()
        check("load: no failed commit", load.errors == 0, load.errors)
        print("== point-in-time restore (DSN-NOD-620, SCN-017)", flush=True)
        pitr(env)
        print("== upgrade (DSN-NOD-633)", flush=True)
        stop_node(node)
        node = None
        upgrade(env)
        node = start_node(env)
        st, r = env.http("GET", "/readyz")
        check("upgrade: the node starts ready afterwards", st == 200, r)
    finally:
        if load:
            load.stop.set()
        if node:
            stop_node(node)
        if getattr(env, "pg_data", None):
            env.run(os.path.join(env.pg_bin, "pg_ctl"), "-D", env.pg_data, "-m", "fast", "-w", "stop", pg=True)
        failed = [c for c in CHECKS if not c["ok"]]
        report = {"suite": "SUITE-NOD-OPS", "part": "PH-1 (step 16f)", "items": ["VER-FAULT-2000"], "exit": "EXIT-1-18",
                  "pass": not failed and bool(CHECKS), "version": getattr(env, "version", None),
                  "host": {"system": platform.platform(), "machine": platform.machine(), "cpus": os.cpu_count(),
                           "postgres": subprocess.run([os.path.join(env.pg_bin, "postgres"), "--version"],
                                                      capture_output=True, text=True).stdout.strip()},
                  "seconds": round(time.time() - started, 1), "steps": STEPS, "checks": CHECKS}
        if a.report:
            os.makedirs(os.path.dirname(os.path.abspath(a.report)), exist_ok=True)
            json.dump(report, open(a.report, "w"), indent=2)
        print(f"\nSUITE-NOD-OPS: {len(CHECKS) - len(failed)} of {len(CHECKS)} checks passed ({report['seconds']} s)")
        if not a.keep:
            shutil.rmtree(a.work, ignore_errors=True)
    sys.exit(1 if failed or not CHECKS else 0)


if __name__ == "__main__":
    main()
