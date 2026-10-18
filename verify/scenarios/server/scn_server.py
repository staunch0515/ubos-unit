#!/usr/bin/env python3
"""The operator scenarios of gate 1b over UBTP on a single-node Server (EXIT-1-19; PH-1 step
18d-3b, DEC-082): SCN-001 (bootstrap a VAE from genesis) and SCN-017 (backup and
point-in-time restore), with their alternate flows.

The node is installed and its PostgreSQL cluster (WAL archiving on) set up by the steps of
SUITE-NOD-OPS (`crates/ubos-node/ops/ops_suite.py`, INSTALL.md); the operator runs the
node's commands (`bootstrap`, `backup`, `restore`, `verify`); clients read through UBTP over
HTTP (`POST /ubtp/v1`) with the administrator's bearer token.

  SCN-001 A2  a genesis Buk whose signature is invalid: bootstrap stops with
              PKG.SIGNATURE_INVALID and writes nothing (UBOS_FAULT_BUK_SIGNATURE)
  SCN-001 A1  bootstrap interrupted after the genesis step, then rerun: completed once
  SCN-001     the tenant, the VAE and `main`; the genesis Buks recorded by one signed commit
              each (phase note of SCN-001); the administrator's DID and key pair; the
              class `Party` through `query class`; the root of `main`
  SCN-017     commits, a restore point, more commits; a restore to the point's time in a
              separate cluster; the heads and DVM roots equal those recorded at that time,
              the projections rebuilt and equal (`verify`); a Server on the restored
              database answers the same root of `main` over UBTP
  SCN-017 A1  a gap in the WAL chain: the restore refuses and reports the last consistent
              point

Usage:
  scn_server.py --binary target/debug/ubos-server [--work DIR] [--report FILE] [--pg-bin DIR]

Standard library only; runs as an ordinary user with the PostgreSQL server binaries
installed (as root, PostgreSQL runs as the `postgres` user).
"""

import argparse
import importlib.util
import json
import os
import platform
import re
import shutil
import sys
import time

HERE = os.path.dirname(os.path.abspath(__file__))
OPS = os.path.join(HERE, "..", "..", "..", "crates", "ubos-node", "ops", "ops_suite.py")
spec = importlib.util.spec_from_file_location("ops_suite", OPS)
ops = importlib.util.module_from_spec(spec)
spec.loader.exec_module(ops)
check = ops.check
STEPS = {}
PUBLISHER_BUKS = ["ontology", "system"]


def ubtp(env, *messages):
    """The frames answering `messages` after a bearer handshake, over HTTP."""
    hs = {"v": "1.0", "id": "h", "type": "handshake",
          "body": {"vae": "acme", "versions": ["1.0"], "credentials": {"kind": "bearer", "token": env.token}}}
    st, r = env.http("POST", "/ubtp/v1", [hs] + list(messages))
    if st != 200 or not isinstance(r, dict):
        return [{"type": "error", "body": {"code": "HTTP", "message": str((st, r))[:300]}}]
    return r["responses"][1:]


def query(env, body, i="q"):
    f = ubtp(env, {"v": "1.0", "id": i, "type": "query", "body": dict({"vae": "acme", "branch": "main"}, **body)})[0]
    return f["body"] if f["type"] == "result" else {"error": f["body"]}


def mutate(env, changes, message=None):
    body = {"vae": "acme", "branch": "main", "commit": {"changes": changes}}
    if message:
        body["message"] = message
    f = ubtp(env, {"v": "1.0", "id": "m", "type": "mutate", "body": body})[0]
    return f["type"] == "result", f["body"]


def head(env):
    r = query(env, {"commit_info": {}})
    return (r.get("items") or [{}])[0]


# ---- SCN-001 ----

def scn001_a2(env):
    r = env.run(env.bin, "bootstrap", "--config", env.cfg, env={"UBOS_FAULT_BUK_SIGNATURE": "ontology"})
    out = r.stdout + r.stderr
    check("SCN-001 A2: an invalid genesis Buk signature stops bootstrap with PKG.SIGNATURE_INVALID",
          r.returncode != 0 and "PKG.SIGNATURE_INVALID" in out, out[-800:])
    t = env.psql("SELECT (SELECT count(*) FROM tenant) + (SELECT count(*) FROM tenant_setup)", db="ubos")
    check("SCN-001 A2: nothing written: no tenant, no setup step, no VAE",
          t.returncode == 0 and t.stdout.strip() == "0", t.stdout + t.stderr)


def scn001_a1(env):
    r = env.run(env.bin, "bootstrap", "--config", env.cfg, env={"UBOS_FAULT_AFTER_STEP": "genesis"})
    check("SCN-001 A1: bootstrap interrupted after the genesis step", r.returncode == 86, r.stdout + r.stderr)
    r = env.run(env.bin, "bootstrap", "--config", env.cfg)
    m = re.search(r"(http://\S+/api/v1/identity/invitations/\S+)", r.stdout)
    steps = dict(re.findall(r"^ok   tenant (\w+): \S+: (already done|done)$", r.stdout, re.M))
    check("SCN-001 A1: the rerun completes the missing steps and leaves the done ones",
          r.returncode == 0 and m and steps.get("genesis") == "already done" and steps.get("projection") == "done",
          r.stdout[-1200:] + r.stderr[-400:])
    env.invitation = m.group(1) if m else None


def scn001_main(env):
    ops.sign_in(env)
    st, me = env.http("GET", "/api/v1/identity/me", token=env.token)
    did = (me.get("principal") or {}).get("did") if isinstance(me, dict) else None
    st2, keys = env.http("GET", f"/api/v1/identity/principals/{did}/keys", token=env.token)
    st3, doc = env.http("GET", f"/api/v1/identity/did/{did}", token=env.token)
    check("SCN-001 step 3: the administrator principal with its DID, role and key pair",
          st == 200 and did and did.startswith("did:ubos:acme:") and (me["principal"].get("role") == "admin")
          and st2 == 200 and st3 == 200 and "verificationMethod" in json.dumps(doc), (me, keys, doc))
    h = head(env)
    check("SCN-001 step 1: the tenant, the default VAE and its branch main answer over UBTP",
          h.get("branch") == "main" and h.get("commit_id", "").startswith("sha256:"), h)
    bukq = query(env, {"query": {"from": "Buk"}})
    items = bukq.get("items") or []
    names = sorted((x.get("body") or {}).get("name", "") for x in items)
    check("SCN-001 step 2: the genesis Buks system and ontology recorded once each (no duplicates)",
          names == PUBLISHER_BUKS, bukq)
    commits = []
    for x in items:
        hist = query(env, {"history": {"target": "ubos://acme/Buk/" + x.get("slug", "")}})
        commits.append([c.get("commit_id") for c in hist.get("items") or []])
    flat = [c for cs in commits for c in cs]
    check("SCN-001 postcondition 1: the history of main shows exactly two genesis installations, one commit each",
          len(commits) == 2 and all(len(cs) == 1 for cs in commits) and len(set(flat)) == 2, commits)
    # each installation commit records the Buk with its content hash and the publisher's
    # signature; the node verifies those manifests against the publisher key
    v = env.run(env.bin, "genesis-verify")
    verified = {ln.split()[1]: ln.split()[3] for ln in v.stdout.splitlines() if ln.startswith("ok   ")}
    recorded = {(x.get("body") or {}).get("name"): x.get("body") or {} for x in items}
    check("SCN-001 postcondition 2: every genesis installation records a Buk that verifies against the publisher key",
          v.returncode == 0 and sorted(verified) == PUBLISHER_BUKS
          and all(recorded.get(n, {}).get("content_hash") == verified[n] and recorded[n].get("signature")
                  and recorded[n].get("publisher") for n in verified), (v.stdout, recorded))
    cls = query(env, {"class": {"name": "Party"}})
    c = (cls.get("items") or [{}])[0]
    check("SCN-001 step 4: the class Party with its inheritance chain and fields over UBTP",
          (c.get("linearization") or [None])[0] == "Party" and "Object" in c["linearization"] and c.get("fields"), cls)
    check("SCN-001 postcondition 3: the Merkle root of main is reported", str(h.get("root", "")).startswith("sha256:"), h)
    STEPS["scn001_root"] = h.get("root")


# ---- SCN-017 ----

def person(n, canary):
    return {"op": "create", "class": "Person", "object_id": "01890a5d-ac96-7017-8000-%012x" % n,
            "slug": f"p{n}", "body": {"name": f"{canary} {n}", "email": f"p{n}@example.com"}}


def scn017(env):
    r = env.run(env.bin, "backup", "base", "--config", env.cfg, timeout=1800)
    check("SCN-017 precondition: continuous backup (a base backup, WAL archived)", r.returncode == 0, r.stderr[-500:])
    ok = all(mutate(env, [person(n, env.canary)], "before the chosen time")[0] for n in range(1, 6))
    chosen = head(env)
    r = env.run(env.bin, "backup", "point", "chosen", "--config", env.cfg)
    point = json.loads(r.stdout) if r.returncode == 0 else {}
    ok = ok and all(mutate(env, [person(n, env.canary)], "after the chosen time")[0] for n in range(6, 11))
    after = head(env)
    check("SCN-017: commits before and after the chosen time", ok and point.get("at") and after.get("root") != chosen.get("root"),
          (point, chosen, after))
    env.psql("SELECT pg_switch_wal()")
    last = json.loads(env.run(env.bin, "backup", "point", "end", "--config", env.cfg).stdout or "{}").get("wal_file", "")
    env.psql("SELECT pg_switch_wal()")
    for _ in range(120):
        archived = env.psql("SELECT coalesce(last_archived_wal, '') FROM pg_stat_archiver").stdout.strip()
        if archived and last and archived[8:] >= last[8:]:
            break
        time.sleep(1)
    # step 1: a restore to the chosen time in a separate environment
    port = ops.free_port()
    t0 = time.time()
    into, r = ops.restore(env, ["--time", point.get("at", "")], port)
    err = (r.stdout + r.stderr) if r.returncode != 0 else ops.start_restored(env, into, port)
    check("SCN-017 step 1: restored to the chosen time in a separate cluster", not err, err)
    STEPS["scn017_restore_seconds"] = round(time.time() - t0, 2)
    if err:
        return
    # steps 2 and 3: heads and roots against those recorded at that time; the projections
    report = os.path.join(env.work, "restore-report.json")
    v = env.run(env.bin, "verify", "--config", env.cfg, "--database", env.ubos_url(port), "--point", "chosen",
                "--last", "1000", "--report", report, timeout=1800)
    vj = json.loads(v.stdout) if v.stdout.strip().startswith("{") else {}
    rep = json.load(open(report)) if os.path.exists(report) else {}
    check("SCN-017 step 2: the heads and DVM roots of the restored branches equal those recorded at the chosen time",
          v.returncode == 0 and vj.get("heads_equal") is True and vj.get("ok") is True, v.stdout[-800:] + v.stderr[-400:])
    checks = rep.get("checks") or []
    check("SCN-017 step 3: the projections rebuilt from the restored commits equal the restored ones",
          checks and all(c.get("ok") is True for c in checks), checks)
    check("SCN-017 postcondition: the restore report lists the heads, roots and verification results",
          rep.get("heads") and checks and "mismatches" in rep, list(rep))
    STEPS["scn017_report"] = {"heads": rep.get("heads"), "checks": checks}
    # a Server on the restored database answers the chosen root over UBTP
    cfg2 = env.cfg + ".restored"
    url2 = os.path.join(os.path.dirname(env.cfg), "database-restored.url")
    open(url2, "w").write(env.ubos_url(port) + "\n")
    http2 = ops.free_port()
    text = open(env.cfg).read().replace(f"127.0.0.1:{env.http_port}", f"127.0.0.1:{http2}")
    text = text.replace(os.path.join(os.path.dirname(env.cfg), "database.url"), url2)
    open(cfg2, "w").write(text)
    saved = (env.cfg, env.url, env.http_port)
    env.cfg, env.url, env.http_port = cfg2, f"http://127.0.0.1:{http2}", http2
    node = ops.start_node(env)
    try:
        h = head(env)
        check("SCN-017: a Server on the restored database answers the root of main recorded at the chosen time over UBTP",
              h.get("root") == chosen.get("root") and h.get("commit_id") == chosen.get("commit_id"), (h, chosen))
    finally:
        ops.stop_node(node)
        env.cfg, env.url, env.http_port = saved
        ops.stop_restored(env, into)
    # A1: a broken chain
    broken = os.path.join(env.work, "backups-broken")
    shutil.copytree(env.backups, broken)
    cfgb = env.cfg + ".broken"
    open(cfgb, "w").write(open(env.cfg).read().replace(env.backups, broken))
    listing = json.loads(env.run(env.bin, "backup", "list", "--config", env.cfg).stdout)
    # a segment missing after the chosen point and before the end point
    chosen_wal = [x for x in listing["points"] if x["name"] == "chosen"][0]["wal_file"]
    end_wal = [x for x in listing["points"] if x["name"] == "end"][0]["wal_file"]
    segs = sorted(f[:24] for f in os.listdir(os.path.join(broken, "wal")) if len(f) == 28)
    gap = [s for s in segs if chosen_wal[8:] < s[8:] <= end_wal[8:]]
    if ops.ROOT:
        import subprocess
        subprocess.run(["chown", "-R", "postgres", broken])
    if not gap:
        check("SCN-017 A1: a WAL segment between the chosen point and the end point to remove", False, segs)
        return
    os.remove(os.path.join(broken, "wal", gap[-1] + ".enc"))
    into, r = ops.restore(env, ["--point", "end"], ops.free_port(), cfg=cfgb)
    d = json.loads(r.stdout) if r.stdout.strip().startswith("{") else {}
    check("SCN-017 A1: a broken backup chain: the restore reports the last consistent point and does not restore past it",
          r.returncode != 0 and d.get("reason") == "wal_chain_broken"
          and (d.get("last_consistent") or {}).get("name") == "chosen", r.stdout + r.stderr)
    shutil.rmtree(into, ignore_errors=True)


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--binary", required=True)
    ap.add_argument("--work", default=None)
    ap.add_argument("--report", default=None)
    ap.add_argument("--pg-bin", default=None)
    ap.add_argument("--keep", action="store_true")
    a = ap.parse_args()
    a.package, a.points, a.spacing = None, 0, 0
    a.work = a.work or (f"/var/tmp/ubos-scn-{os.getpid()}" if ops.ROOT else os.path.join(os.getcwd(), f"ubos-scn-{os.getpid()}"))
    if os.path.exists(a.work):
        sys.exit(f"{a.work} exists")
    os.makedirs(a.work)
    env = ops.Env(a)
    started = time.time()
    node = None
    try:
        print("== install and PostgreSQL (SUITE-NOD-OPS steps)", flush=True)
        ops.install(env)
        if not ops.postgres(env):
            raise SystemExit
        print("== SCN-001", flush=True)
        scn001_a2(env)
        scn001_a1(env)
        node = ops.start_node(env)
        scn001_main(env)
        print("== SCN-017", flush=True)
        scn017(env)
    finally:
        if node:
            ops.stop_node(node)
        if getattr(env, "pg_data", None):
            env.run(os.path.join(env.pg_bin, "pg_ctl"), "-D", env.pg_data, "-m", "fast", "-w", "stop", pg=True)
        checks = [c for c in ops.CHECKS if c["check"].startswith("SCN-")]
        setup = [c for c in ops.CHECKS if not c["check"].startswith("SCN-")]
        failed = [c for c in ops.CHECKS if not c["ok"]]
        report = {"exit": "EXIT-1-19", "part": "PH-1 step 18d-3b", "scenarios": ["SCN-001", "SCN-017"],
                  "binding": "http", "pass": not failed and bool(checks), "version": getattr(env, "version", None),
                  "host": {"system": platform.platform(), "machine": platform.machine(), "cpus": os.cpu_count()},
                  "seconds": round(time.time() - started, 1), "steps": STEPS, "checks": checks, "setup": setup}
        if a.report:
            os.makedirs(os.path.dirname(os.path.abspath(a.report)), exist_ok=True)
            json.dump(report, open(a.report, "w"), indent=2)
        print(f"\nSCN-001, SCN-017: {len(checks) - len([c for c in checks if not c['ok']])} of {len(checks)} checks passed"
              f" ({report['seconds']} s)")
        if not a.keep:
            shutil.rmtree(a.work, ignore_errors=True)
    sys.exit(1 if failed or not checks else 0)


if __name__ == "__main__":
    main()
