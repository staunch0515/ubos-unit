#!/usr/bin/env python3
"""The developer loop of `forge dev` (NR-PERF-016, DSN-FRG-501; PH-1 step 17e, DEC-078).

Generates the reference project of NR-PERF-016 (100 classes, 200 rules, 50 tests; one logic
asset behind a port of every class), starts `forge dev --test` on it and measures:

- the start: until the node serves the installed project;
- the loop, `--edits` times: a script is saved with a new result, and a UBTP client calls
  the port on the development branch until the new result comes back (save → the running
  VAE uses it, DSN-FRG-501 acceptance 1); and the whole edit–check–test loop of NR-PERF-016
  as forge reports it (check, install and the 50 tests);
- a model change: a field is added, the development VAE is rebuilt and serves again.

Usage:
  python3 verify/tools/dev_loop.py --forge target/release/forge --server target/release/ubos-server \\
      [--database "host=127.0.0.1 user=postgres dbname=bpa"] [--edits 10] [--report FILE]
"""
import argparse
import datetime
import json
import os
import platform
import re
import secrets
import shutil
import statistics
import subprocess
import sys
import tempfile
import threading
import time
import urllib.request

RULES_PER_CLASS, TESTS = 2, 50
CLASSES = 100


def write(path, text):
    os.makedirs(os.path.dirname(path), exist_ok=True)
    open(path, "w").write(text)


def asset(version):
    return {"name": "ref.logic", "abi": "1.0", "tier": "host", "profile": "standard", "state": "committed",
            "capabilities": ["read"], "exports": ["describe", "double"],
            "source": {"functions": {
                "describe": {"params": [], "body": [{"do": "read.get", "as": "me", "args": {"target": "self_id"}},
                                                    {"return": f'concat("v{version} ", me.fields.name)'}]},
                "double": {"params": ["n"], "body": [{"return": "params.n * 2"}]}}}}


def oid(n):
    return f"01890a5d-ac96-7001-8000-{n:012x}"


def project(root, extra_field=False, classes=None):
    global CLASSES
    if classes:
        CLASSES = classes
    write(f"{root}/forge.toml", '[project]\nname = "reference"\nversion = "0.1.0"\nvae = "acme"\nabi = "1.0"\nbranch = "main"\n')
    lines = ["classes:"]
    for i in range(CLASSES):
        fields = [{"name": "name", "label": "name", "description": "name", "type": "text", "required": True},
                  {"name": "amount", "label": "amount", "description": "amount", "type": "decimal"},
                  {"name": "rating", "label": "rating", "description": "rating", "type": "integer"},
                  {"name": "status", "label": "status", "description": "status", "type": "text"}]
        if extra_field and i == 0:
            fields.append({"name": "note", "label": "note", "description": "note", "type": "text"})
        rules = [{"name": "rating_range", "message": "rating is 0 to 10", "severity": "error",
                  "rule": "this.rating is null or (this.rating >= 0 and this.rating <= 10)"},
                 {"name": "amount_positive", "message": "amount is not negative", "severity": "error",
                  "rule": "this.amount is null or this.amount >= 0"}][:RULES_PER_CLASS]
        c = {"name": f"C{i:03d}", "label": f"C{i:03d}", "description": f"class {i}", "namespace": "ref.app",
             "parent": "DataObject", "kind": "definition", "schema_version": 1, "fields": fields, "validations": rules,
             "ports": [{"name": "describe", "description": "describe", "kind": "command", "params": [],
                        "permission": "describe", "impl": "ref.logic#describe"}]}
        lines.append("  - " + json.dumps(c))
    write(f"{root}/model/classes.yaml", "\n".join(lines) + "\n")
    write(f"{root}/logic/ref.yaml", "assets:\n  - " + json.dumps(asset(1)) + "\n")
    seed = [{"op": "create", "class": f"C{i:03d}", "object_id": oid(i + 1), "slug": f"c{i:03d}",
             "body": {"name": f"object {i}", "amount": "10", "rating": 3}} for i in range(CLASSES)]
    write(f"{root}/seed/objects.yaml", "changes:\n" + "".join("  - " + json.dumps(c) + "\n" for c in seed))
    tests = []
    for i in range(TESTS // 2):
        tests.append({"name": f"object {i} reads back", "kind": "scenario", "steps": [
            {"op": "read", "input": {"branch": "main", "object_id": oid(i + 1)},
             "expect": {"match": "subset", "result": {"body": {"name": f"object {i}"}}}}]})
        tests.append({"name": f"double {i}", "kind": "unit", "asset": "ref.logic", "function": "double",
                      "args": [i], "expect": {"result": 2 * i}})
    write(f"{root}/tests/reference.yaml", "tests:\n" + "".join("  - " + json.dumps(t) + "\n" for t in tests))


class Ubtp:
    """A UBTP client over HTTP with dev credentials."""

    def __init__(self, url):
        self.url, self.session = url, None
        r = self.send([{"v": "1.0", "id": "h", "type": "handshake",
                        "body": {"vae": "acme", "versions": ["1.0"], "credentials": {"kind": "dev"}}}])
        assert r[0]["type"] == "result", r

    def send(self, messages):
        req = urllib.request.Request(self.url + "/ubtp/v1", data=json.dumps(messages).encode(), method="POST")
        req.add_header("content-type", "application/json")
        if self.session:
            req.add_header("ubtp-session", self.session)
        with urllib.request.urlopen(req, timeout=10) as r:
            self.session = r.headers.get("ubtp-session") or self.session
            return [f for f in json.loads(r.read())["responses"] if f["type"] != "emit"]

    def describe(self, cls="C000", slug="c000"):
        r = self.send([{"v": "1.0", "id": secrets.token_hex(4), "type": "mutate",
                        "body": {"vae": "acme", "branch": "feature/dev", "idempotency_key": secrets.token_hex(8),
                                 "port": {"target": f"ubos://acme/{cls}/{slug}", "name": "describe", "params": {}}}}])
        f = r[0]
        return f["body"].get("result") if f["type"] == "result" else f["body"].get("code")


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--forge", required=True)
    ap.add_argument("--server", required=True)
    ap.add_argument("--database")
    ap.add_argument("--edits", type=int, default=10)
    ap.add_argument("--report")
    ap.add_argument("--target-ms", type=int, default=10000)
    args = ap.parse_args()
    root = tempfile.mkdtemp(prefix="dev-loop-")
    project(root)
    port = 17000 + secrets.randbelow(2000)
    cmd = [args.forge, "dev", root, "--json", "--test", "--listen", f"127.0.0.1:{port}", "--server", args.server]
    if args.database:
        db = "ubos_devloop_" + secrets.token_hex(3)
        subprocess.run(["psql", args.database, "-Atc", f"CREATE DATABASE {db}"], check=True, capture_output=True)
        cmd += ["--database", re.sub(r"dbname=\S+", f"dbname={db}", args.database)]
    t_start = time.time()
    proc = subprocess.Popen(cmd, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True)
    events = []
    cond = threading.Condition()

    def reader():
        for line in proc.stdout:
            try:
                e = json.loads(line)
            except ValueError:
                print("  forge:", line.rstrip())
                continue
            with cond:
                events.append(e)
                cond.notify_all()
    threading.Thread(target=reader, daemon=True).start()

    def wait_event(kind, after, timeout=120):
        with cond:
            end = time.time() + timeout
            while True:
                for e in events[after:]:
                    if e.get("event") == kind:
                        return e
                    if e.get("event") in ("rejected", "install_failed"):
                        sys.exit(f"forge dev: {e}")
                left = end - time.time()
                if left <= 0:
                    sys.exit(f"no {kind} event: {events[-3:]}")
                cond.wait(left)

    ready = wait_event("ready", 0)
    start_s = round(time.time() - t_start, 2)
    c = Ubtp(ready["url"])
    first = c.describe()
    print(f"ready in {start_s} s ({ready['ms']} ms inside forge dev, tests {ready['tests']}); describe -> {first!r}")
    loops = []
    for k in range(2, args.edits + 2):
        n = len(events)
        write(f"{root}/logic/ref.yaml", "assets:\n  - " + json.dumps(asset(k)) + "\n")
        t0 = time.time()
        want = f"v{k} object 0"
        while True:
            got = c.describe()
            if got == want:
                break
            if time.time() - t0 > 60:
                sys.exit(f"edit {k}: still {got!r}")
            time.sleep(0.02)
        ms = round((time.time() - t0) * 1000)
        e = wait_event("reloaded", n)
        loops.append({"edit": k, "save_to_use_ms": ms, "check_ms": e["check_ms"], "install_ms": e["install_ms"],
                      "forge_ms": e["ms"], "tests": e["tests"]})
        print(f"edit {k}: the running VAE uses it after {ms} ms (check {e['check_ms']} ms, tests "
              f"{e['tests'].get('test_ms')} ms, install {e['install_ms']} ms; {e['tests'].get('tests')} tests, "
              f"{len(e['tests'].get('failed', []))} failed)")
    # a model change rebuilds the development VAE
    n = len(events)
    project(root, extra_field=True)
    write(f"{root}/logic/ref.yaml", "assets:\n  - " + json.dumps(asset(args.edits + 1)) + "\n")
    t0 = time.time()
    e = wait_event("rebuilt", n)
    c = Ubtp(ready["url"])
    got = c.describe()
    rebuild_s = round(time.time() - t0, 2)
    print(f"model change: rebuilt and serving after {rebuild_s} s; describe -> {got!r}")
    proc.terminate()
    proc.wait(30)
    if args.database:
        subprocess.run(["psql", args.database, "-Atc", f"DROP DATABASE IF EXISTS {db} WITH (FORCE)"], capture_output=True)
    shutil.rmtree(root, ignore_errors=True)
    # `forge check` of a 500-class project after a script change (DSN-FRG-003 acceptance)
    big = tempfile.mkdtemp(prefix="dev-loop-500-")
    project(big, classes=500)
    write(f"{big}/logic/ref.yaml", "assets:\n  - " + json.dumps(asset(99)) + "\n")
    t0 = time.time()
    subprocess.run([args.forge, "check", big], check=True, capture_output=True)
    check500 = round((time.time() - t0) * 1000)
    shutil.rmtree(big, ignore_errors=True)
    CLASSES_REF = 100
    lat = [x["save_to_use_ms"] for x in loops]
    # the whole loop: the save is seen, checked, installed and the tests have run
    full = [x["forge_ms"] for x in loops]
    q = statistics.quantiles(lat, n=20) if len(lat) >= 2 else lat
    summary = {"reference": {"classes": CLASSES_REF, "rules": CLASSES_REF * RULES_PER_CLASS, "tests": TESTS, "assets": 1},
               "backend": "postgres" if args.database else "memory", "start_s": start_s,
               "save_to_use_ms": {"p50": statistics.median(lat), "p95": q[-1] if len(lat) >= 2 else lat[0], "max": max(lat)},
               "edit_check_test_ms": {"p50": statistics.median(full), "max": max(full)},
               "model_change_rebuild_s": rebuild_s, "target_ms": args.target_ms,
               "check_500_classes_ms": check500,
               "met": max(full) <= args.target_ms and max(lat) <= args.target_ms and all(not x["tests"].get("failed") for x in loops)}
    report = {"benchmark": "NR-PERF-016 developer loop (forge dev)", "step": "PH-1 17e (DEC-078)",
              "date": datetime.datetime.now(datetime.timezone.utc).isoformat(timespec="seconds"),
              "host": {"machine": platform.machine(), "cpus": os.cpu_count(), "system": platform.system()},
              "summary": summary, "edits": loops, "rebuild_event": e}
    print(json.dumps(summary, indent=1))
    if args.report:
        json.dump(report, open(args.report, "w"), indent=1)
        open(args.report, "a").write("\n")
    return 0 if summary["met"] else 1


if __name__ == "__main__":
    sys.exit(main())
