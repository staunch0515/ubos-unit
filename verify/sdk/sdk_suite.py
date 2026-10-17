#!/usr/bin/env python3
"""SUITE-SDK-PROTO, SUITE-SDK-NET and SUITE-SDK-TYPES for every SDK (PH-1 step 17d, DEC-078).

Starts a node on a fresh PostgreSQL database with the classes of verify/sdk/fixture (and the
branch `feature/sdk` holding the fixture's logic), puts a TCP fault proxy in front of it, and runs
each SDK's runner over each binding through the proxy. The runners implement the cases of
verify/sdk/cases.json and drive the proxy through its control endpoint:

  POST /fault?mode=none | drop | cut_reply | cut_after&bytes=N | latency&ms=N | partition&ms=N
  POST /stats      {"cuts", "connections"}
  POST /restart    the node is restarted; answers when it is ready

SUITE-SDK-TYPES (TypeScript): the committed generated types are current, a program narrowing
a polymorphic query compiles, and the same program fails to compile against the types of a
model without a field it uses (DSN-SDK-012, DSN-FRG-102).

Usage:
  python3 verify/sdk/sdk_suite.py --server target/debug/ubos-server --forge target/debug/forge \\
      --rust target/debug/examples/suite --ts packages/sdk-ts/build/suite/suite.js \\
      --database "host=127.0.0.1 user=postgres dbname=bpa" [--report FILE] [--bindings ws,http]
"""
import argparse
import asyncio
import datetime
import json
import os
import platform
import re
import secrets
import shutil
import signal
import subprocess
import sys
import tempfile
import threading
import time
import urllib.request

ROOT = os.path.abspath(os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", ".."))
FIXTURE = os.path.join(ROOT, "verify", "sdk", "fixture")
PASSWORD = "correct horse battery staple"


def fixture_list(name, key):
    out = []
    for line in open(os.path.join(FIXTURE, name, f"{key}.yaml"), encoding="utf-8"):
        if line.startswith("  - "):
            out.append(json.loads(line[4:]))
    return out


# ---- the fault proxy ----

class Conn(dict):
    """A proxied connection: its writers and flags, compared by identity."""
    __hash__ = object.__hash__
    __eq__ = object.__eq__


class Proxy:
    def __init__(self, target):
        self.target = target
        self.mode = "none"
        self.ms = 0
        self.armed_bytes = None
        self.sent_since_arm = 0
        self.partition_until = 0.0
        self.held = set()
        self.conns = set()
        self.cuts = 0
        self.connections = 0

    def partitioned(self):
        return time.monotonic() < self.partition_until

    def kill(self, conn):
        conn["dead"] = True
        for w in (conn["cw"], conn.get("uw")):
            if w is not None:
                try:
                    w.transport.abort()
                except Exception:
                    pass

    def drop_all(self):
        for c in list(self.conns):
            self.kill(c)

    def heal(self):
        self.partition_until = 0.0
        for c in list(self.held):
            self.kill(c)
        self.held.clear()

    def fault(self, mode, ms=0, nbytes=0):
        if mode == "drop":
            self.drop_all()
            self.mode = "none"
        elif mode == "partition":
            self.partition_until = time.monotonic() + ms / 1000
            self.held = set(self.conns)
            asyncio.get_running_loop().call_later(ms / 1000 + 0.01, self.heal_if_due)
            self.mode = "none"
        elif mode == "cut_after":
            self.mode, self.armed_bytes, self.sent_since_arm = "cut_after", nbytes, 0
        elif mode == "latency":
            self.mode, self.ms = ("latency", ms) if ms > 0 else ("none", 0)
        elif mode == "none":
            self.mode, self.ms = "none", 0
            if self.partition_until:
                self.heal()
        else:
            self.mode = mode
        return {"mode": self.mode, "cuts": self.cuts}

    def heal_if_due(self):
        if self.partition_until and not self.partitioned():
            self.heal()

    async def pump(self, conn, reader, writer, upstream):
        try:
            while not conn["dead"]:
                data = await reader.read(65536)
                if not data:
                    break
                while self.partitioned() and not conn["dead"]:
                    if conn not in self.held:
                        self.held.add(conn)
                    await asyncio.sleep(0.05)
                if conn["dead"]:
                    break
                if upstream:
                    if self.mode == "cut_reply":
                        self.mode = "none"
                        conn["cutting"] = True
                        writer.write(data)
                        await writer.drain()
                        self.cuts += 1
                        await asyncio.sleep(0.5)
                        self.kill(conn)
                        break
                    if self.mode == "cut_after":
                        self.sent_since_arm += len(data)
                        if self.sent_since_arm > self.armed_bytes:
                            self.mode = "none"
                            self.cuts += 1
                            self.kill(conn)
                            break
                elif conn.get("cutting"):
                    continue
                if self.mode == "latency":
                    await asyncio.sleep(self.ms / 1000)
                writer.write(data)
                await writer.drain()
        except Exception:
            pass
        finally:
            self.kill(conn)

    async def handle(self, cr, cw):
        conn = Conn(cw=cw, uw=None, dead=False)
        self.conns.add(conn)
        self.connections += 1
        if self.partitioned():
            self.held.add(conn)
        try:
            while self.partitioned() and not conn["dead"]:
                await asyncio.sleep(0.05)
            if conn["dead"]:
                return
            ur, uw = await asyncio.open_connection("127.0.0.1", self.target)
            conn["uw"] = uw
            await asyncio.gather(self.pump(conn, cr, uw, True), self.pump(conn, ur, cw, False))
        except Exception:
            pass
        finally:
            self.kill(conn)
            self.conns.discard(conn)
            self.held.discard(conn)


# ---- the node ----

class Node:
    def __init__(self, args, work):
        self.args, self.work = args, work
        self.db = "ubos_sdk_" + secrets.token_hex(4)
        self.conn = re.sub(r"dbname=\S+", f"dbname={self.db}", args.database)
        self.port = free_port()
        self.url = f"http://127.0.0.1:{self.port}"
        self.env = dict(os.environ, UBOS_ROOT_KEK=secrets.token_hex(32))
        self.proc = None
        self.log = open(os.path.join(work, "server.log"), "a")

    def sql(self, q):
        return subprocess.run(["psql", self.args.database, "-Atc", q], capture_output=True, text=True)

    def setup(self):
        self.sql(f"DROP DATABASE IF EXISTS {self.db} WITH (FORCE)")
        r = self.sql(f"CREATE DATABASE {self.db}")
        if r.returncode:
            sys.exit(f"cannot create the database: {r.stderr}")
        self.cfg = os.path.join(self.work, "ubos.toml")
        open(self.cfg, "w").write(f'version = 1\n[server]\nlisten = "127.0.0.1:{self.port}"\npreset = "server"\n'
                                  f'tenant = "sdk"\nvae = "acme"\n[database]\nurl = "{self.conn}"\n'
                                  f'[keys]\nroot_kek = "env:UBOS_ROOT_KEK"\n')
        genesis = os.path.join(self.work, "genesis.json")
        json.dump({"classes": fixture_list("model", "classes"), "branches": ["feature/sdk"]}, open(genesis, "w"))
        b = subprocess.run([self.args.server, "bootstrap", "--config", self.cfg, "--genesis", genesis],
                           env=self.env, capture_output=True, text=True)
        m = re.search(r"(http://\S+/api/v1/identity/invitations/\S+)", b.stdout)
        if b.returncode or not m:
            sys.exit(f"bootstrap failed: {b.stdout}{b.stderr}")
        self.start()
        post(m.group(1), {"password": PASSWORD})

    def start(self):
        self.proc = subprocess.Popen([self.args.server, "--config", self.cfg], env=self.env,
                                     stdout=self.log, stderr=subprocess.STDOUT)
        for _ in range(300):
            try:
                if urllib.request.urlopen(self.url + "/readyz", timeout=2).status == 200:
                    return
            except Exception:
                time.sleep(0.1)
        sys.exit("the node did not become ready")

    def stop(self):
        if self.proc and self.proc.poll() is None:
            self.proc.send_signal(signal.SIGTERM)
            try:
                self.proc.wait(30)
            except subprocess.TimeoutExpired:
                self.proc.kill()

    def restart(self):
        self.stop()
        time.sleep(0.5)
        self.start()

    def drop(self):
        self.stop()
        self.sql(f"DROP DATABASE IF EXISTS {self.db} WITH (FORCE)")


def free_port():
    import socket
    s = socket.socket()
    s.bind(("127.0.0.1", 0))
    p = s.getsockname()[1]
    s.close()
    return p


def post(url, body, headers=None, method="POST"):
    req = urllib.request.Request(url, data=json.dumps(body).encode(), method=method)
    req.add_header("content-type", "application/json")
    for k, v in (headers or {}).items():
        req.add_header(k, v)
    with urllib.request.urlopen(req, timeout=30) as r:
        return json.loads(r.read() or b"null"), dict(r.headers)


def credentials(node):
    signed, _ = post(node.url + "/api/v1/identity/sign-in", {"username": "admin", "password": PASSWORD})
    token, did = signed["token"], signed["principal"]
    if isinstance(did, dict):
        did = did.get("did") or did.get("id")
    # the runners open many clients of one account
    post(node.url + "/api/v1/identity/session-policy", {"kind": "human", "max_sessions": 1000},
         {"authorization": f"Bearer {token}"}, "PUT")
    key, _ = post(f"{node.url}/api/v1/identity/principals/{did}/api-keys",
                  {"name": "sdk-suite", "scopes": ["read", "write"]}, {"authorization": f"Bearer {token}"})
    # the fixture's logic on the branch `feature/sdk` (protected branches run verified logic only)
    hs = {"v": "1.0", "id": "h", "type": "handshake",
          "body": {"vae": "acme", "versions": ["1.0"], "credentials": {"kind": "bearer", "token": token}}}
    _, h = post(node.url + "/ubtp/v1", [hs])
    session = {k.lower(): v for k, v in h.items()}["ubtp-session"]
    changes = [{"op": "create", "class": "Logic", "object_id": f"01890a5d-ac96-7001-8000-{i:012x}",
                "slug": a["name"].replace(".", "-"), "body": a} for i, a in enumerate(fixture_list("logic", "assets"), 1)]
    m = {"v": "1.0", "id": "m", "type": "mutate",
         "body": {"vae": "acme", "branch": "feature/sdk", "commit": {"changes": changes}, "idempotency_key": secrets.token_hex(8)}}
    r, _ = post(node.url + "/ubtp/v1", [m], {"ubtp-session": session})
    if r["responses"][0]["type"] != "result":
        sys.exit(f"the fixture logic was refused: {r}")
    return token, key["key"]


# ---- the control endpoint ----

async def control(proxy, node, reader, writer):
    try:
        line = (await reader.readline()).decode()
        while (await reader.readline()) not in (b"\r\n", b"\n", b""):
            pass
        path = line.split(" ")[1] if " " in line else "/"
        q = dict(p.split("=", 1) for p in path.split("?", 1)[1].split("&")) if "?" in path else {}
        if path.startswith("/fault"):
            out = proxy.fault(q.get("mode", "none"), int(q.get("ms", 0)), int(q.get("bytes", 0)))
        elif path.startswith("/stats"):
            out = {"cuts": proxy.cuts, "connections": proxy.connections}
        elif path.startswith("/restart"):
            proxy.drop_all()
            await asyncio.get_running_loop().run_in_executor(None, node.restart)
            out = {"restarted": True}
        else:
            out = {"error": "unknown"}
        body = json.dumps(out).encode()
        writer.write(b"HTTP/1.0 200 OK\r\ncontent-type: application/json\r\ncontent-length: "
                     + str(len(body)).encode() + b"\r\n\r\n" + body)
        await writer.drain()
    finally:
        writer.close()


def serve(proxy, node, ports, ready):
    async def main():
        await asyncio.start_server(proxy.handle, "127.0.0.1", ports[0])
        await asyncio.start_server(lambda r, w: control(proxy, node, r, w), "127.0.0.1", ports[1])
        ready.set()
        await asyncio.Event().wait()
    asyncio.run(main())


# ---- SUITE-SDK-TYPES ----

NARROWING = '''import { connect } from "%(sdk)s";
import { Customer, Party } from "%(types)s";
export async function use(url: string): Promise<string[]> {
  const c = await connect({ url, vae: "acme", auth: { kind: "dev" } });
  const out: string[] = [];
  for await (const p of Party.query().filter("country", "==", "x").run(c)) {
    if (p.class === "Customer") out.push(p.body.email ?? "", p.body.credit_limit?.amount ?? "");
    else if (p.class === "Supplier") out.push(p.body.iban ?? "");
    else out.push(p.body.name);
  }
  const one = await Customer.get(c, "acme");
  out.push(one.body.email ?? "");
  void Customer.query().filter("email", "==", "a@b.c");
  return out;
}
'''


def types_suite(args, work):
    checks = []
    tsc = os.path.join(ROOT, "packages", "sdk-ts", "node_modules", ".bin", "tsc")
    sdk = os.path.join(ROOT, "packages", "sdk-ts", "src", "index.js")
    committed = os.path.join(ROOT, "packages", "sdk-ts", "suite", "generated", "types.ts")
    fresh = os.path.join(work, "fresh")
    subprocess.run([args.forge, "types", FIXTURE, "--out", fresh, "--sdk", "../../src/index.js"], check=True, capture_output=True)
    checks.append({"name": "the committed TypeScript types of the fixture are current (forge types)",
                   "ok": open(committed).read() == open(os.path.join(fresh, "types.ts")).read(), "detail": ""})
    rust_committed = os.path.join(FIXTURE, "generated", "types.rs")
    fresh2 = os.path.join(work, "fresh-rs")
    subprocess.run([args.forge, "types", FIXTURE, "--out", fresh2], check=True, capture_output=True)
    checks.append({"name": "the committed Rust types of the fixture are current (forge types)",
                   "ok": open(rust_committed).read() == open(os.path.join(fresh2, "types.rs")).read(), "detail": ""})

    def compile_with(model_dir, label):
        out = os.path.join(work, label)
        subprocess.run([args.forge, "types", model_dir, "--out", out, "--sdk", sdk], check=True, capture_output=True)
        prog = os.path.join(out, "use.ts")
        open(prog, "w").write(NARROWING % {"sdk": sdk.replace(".js", ".js"), "types": "./types.js"})
        cfg = os.path.join(out, "tsconfig.json")
        json.dump({"extends": os.path.join(ROOT, "packages", "sdk-ts", "tsconfig.json"),
                   "compilerOptions": {"noEmit": True, "rootDir": "/", "types": []},
                   "include": [prog, os.path.join(out, "types.ts")]}, open(cfg, "w"))
        return subprocess.run([tsc, "-p", cfg], capture_output=True, text=True)

    r = compile_with(FIXTURE, "narrowing")
    checks.append({"name": "a polymorphic query narrows by class in the type checker (DSN-SDK-012)",
                   "ok": r.returncode == 0, "detail": (r.stdout + r.stderr)[:600]})
    removed = os.path.join(work, "removed")
    shutil.copytree(FIXTURE, removed, ignore=shutil.ignore_patterns("generated"))
    p = os.path.join(removed, "model", "classes.yaml")
    lines = []
    for line in open(p):
        if line.startswith("  - ") and '"name": "Customer"' in line:
            c = json.loads(line[4:])
            c["fields"] = [f for f in c["fields"] if f["name"] != "email"]
            line = "  - " + json.dumps(c) + "\n"
        lines.append(line)
    open(p, "w").write("".join(lines))
    r = compile_with(removed, "removed-field")
    out = r.stdout + r.stderr
    checks.append({"name": "the same program fails to compile once the model removes a field it uses (DSN-FRG-102)",
                   "ok": r.returncode != 0 and "email" in out, "detail": out[:600]})
    return {"case": "T01", "ok": all(c["ok"] for c in checks), "checks": checks}


# ---- main ----

def run_runner(cmd, label):
    t0 = time.time()
    p = subprocess.run(cmd, capture_output=True, text=True, timeout=1800)
    cases = {}
    for line in p.stdout.splitlines():
        if line.startswith('{"case"'):
            r = json.loads(line)
            cases[r["case"]] = r
    for c in sorted(cases.values(), key=lambda r: r["case"]):
        print(f"  {'ok  ' if c['ok'] else 'FAIL'} {label} {c['case']} ({c.get('ms', 0)} ms)", flush=True)
        for k in c["checks"]:
            if not k["ok"]:
                print(f"         - {k['name']}: {k['detail']}", flush=True)
    if p.returncode and not cases:
        print(p.stdout[-2000:], p.stderr[-2000:])
    return cases, round(time.time() - t0, 1)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--server", required=True)
    ap.add_argument("--forge")
    ap.add_argument("--rust")
    ap.add_argument("--ts")
    ap.add_argument("--database", required=True)
    ap.add_argument("--report")
    ap.add_argument("--bindings", default="ws,http")
    ap.add_argument("--cases")
    args = ap.parse_args()
    manifest = json.load(open(os.path.join(ROOT, "verify", "sdk", "cases.json")))
    ids = [c["id"] for s in manifest["suites"].values() for c in s]
    work = tempfile.mkdtemp(prefix="sdk-suite-")
    node = Node(args, work)
    node.setup()
    token, api_key = credentials(node)
    proxy = Proxy(node.port)
    ports = (free_port(), free_port())
    ready = threading.Event()
    threading.Thread(target=serve, args=(proxy, node, ports, ready), daemon=True).start()
    ready.wait(10)
    t0 = time.time()
    results, failed = {}, 0
    sdks = [("rust", [args.rust]) if args.rust else None, ("typescript", ["node", args.ts]) if args.ts else None]
    for n, (sdk, cmd) in enumerate(s for s in sdks if s):
        for m, binding in enumerate(args.bindings.split(",")):
            label = f"{sdk}/{binding}"
            print(f"== {label}", flush=True)
            argv = cmd + ["--url", f"http://127.0.0.1:{ports[0]}", "--direct", node.url, "--control", f"http://127.0.0.1:{ports[1]}",
                          "--vae", "acme", "--user", "admin", "--password", PASSWORD, "--api-key", api_key, "--token", token,
                          "--binding", binding, "--tag", f"{sdk[:2]}{binding}{secrets.token_hex(3)}"]
            if args.cases:
                argv += ["--cases", args.cases]
            cases, secs = run_runner(argv, label)
            want = args.cases.split(",") if args.cases else ids
            missing = [i for i in want if i not in cases]
            bad = [i for i in want if i in cases and not cases[i]["ok"]]
            failed += len(missing) + len(bad)
            results[label] = {"seconds": secs, "cases": cases, "missing": missing, "failed": bad}
            print(f"  {label}: {len(want) - len(missing) - len(bad)} of {len(want)} cases pass ({secs} s)", flush=True)
    types = None
    if args.forge and args.ts:
        print("== SUITE-SDK-TYPES (TypeScript)", flush=True)
        types = types_suite(args, work)
        for k in types["checks"]:
            print(f"  {'ok  ' if k['ok'] else 'FAIL'} {k['name']}" + ("" if k["ok"] else f": {k['detail']}"), flush=True)
        failed += 0 if types["ok"] else 1
    node.drop()
    summary = {label: {suite: f"{sum(1 for c in cs if r['cases'].get(c['id'], {}).get('ok'))} of {len(cs)}"
                       for suite, cs in manifest["suites"].items()} for label, r in results.items()}
    report = {"suite": "SUITE-SDK-PROTO, SUITE-SDK-NET, SUITE-SDK-TYPES", "step": "PH-1 17d (DEC-078)",
              "date": datetime.datetime.now(datetime.timezone.utc).isoformat(timespec="seconds"),
              "host": {"machine": platform.machine(), "system": platform.system(), "python": platform.python_version()},
              "node": "ubos-server, preset server, PostgreSQL, behind the fault proxy of verify/sdk/sdk_suite.py",
              "summary": summary, "types": types, "results": results, "seconds": round(time.time() - t0, 1),
              "failed": failed}
    if args.report:
        json.dump(report, open(args.report, "w"), indent=1, sort_keys=True)
        open(args.report, "a").write("\n")
    print(json.dumps(summary, indent=1))
    print(f"failed: {failed}")
    shutil.rmtree(work, ignore_errors=True)
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main())
