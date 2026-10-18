#!/usr/bin/env python3
"""Secret hygiene and security-event coverage (VER-SEC-2001; PH-1 step 19a, DEC-085):
NR-SEC-009 (no secret in logs, traces, errors or diagnostics), NR-SEC-010 and FR-AUD-012
(every security event type of PH-1 recorded), DSN-NOD-111, DSN-NOD-130.

A single-node Server is installed and bootstrapped by the steps of SUITE-NOD-OPS
(`ops_suite.py`) with telemetry exported to its log (`telemetry.export = "stderr"`) at level
`debug`. The suite seeds canary secrets — the administrator's and a user's passwords, a TOTP
secret, session tokens, an API key, secret-store values, the root and backup keys and the
database password — and runs a security scenario set over the identity, operator and UBTP
interfaces: sign-in successes and failures with lockout, a second factor, sessions opened and
revoked, principals created and disabled, keys created, rotated and used, API keys issued,
rejected and revoked, secrets stored and deleted, configuration applied and rejected, tenant
settings, a policy committed and a request it denies, a diagnostic bundle, a UBTP workload
with failed requests. It then scans the node's log (logs and spans), the telemetry span
buffer, the diagnostic bundle, the security-log export and every response a client received
for each canary, and compares the event kinds of the tenant's security log with the PH-1
list of FR-AUD-012.

Usage:
  hygiene_suite.py --binary target/debug/ubos-server [--work DIR] [--report FILE] [--pg-bin DIR]

Standard library only.
"""

import argparse
import base64
import hashlib
import hmac
import importlib.util
import json
import os
import platform
import secrets
import shutil
import struct
import sys
import time

HERE = os.path.dirname(os.path.abspath(__file__))
spec = importlib.util.spec_from_file_location("ops_suite", os.path.join(HERE, "ops_suite.py"))
ops = importlib.util.module_from_spec(spec)
spec.loader.exec_module(ops)
check = ops.check

# FR-AUD-012 categories and the PH-1 event kinds that record them; break-glass access is
# PH-2 (DSN-NOD-112)
EXPECTED = {
    "authentication": ["auth.signed_in", "auth.failed", "auth.throttled", "auth.totp_required",
                       "api_key.rejected"],
    "session": ["session.opened", "session.revoked", "policy.session"],
    "permission and policy changes": ["principal.created", "principal.state", "credential.password_set",
                                      "dvm.security:policy.change", "dvm.security:authz.deny"],
    "key operations": ["key.created", "key.rotated", "key.signed", "api_key.issued", "api_key.revoked",
                       "credential.totp_enrolled", "credential.totp_enabled"],
    "administrative actions": ["invitation.created", "invitation.accepted", "secret.put", "secret.deleted",
                               "config.applied", "config.rejected", "tenant.settings"],
    "exports": ["diagnostics.bundle"],
}

CORPUS = []        # every response body a client received


def totp(secret_b32, at=None):
    key = base64.b32decode(secret_b32 + "=" * (-len(secret_b32) % 8))
    counter = int((at or time.time()) // 30)
    h = hmac.new(key, struct.pack(">Q", counter), hashlib.sha1).digest()
    o = h[19] & 0x0F
    code = (struct.unpack(">I", h[o:o + 4])[0] & 0x7FFFFFFF) % 1_000_000
    return "%06d" % code


def call(env, method, path, body=None, token=None):
    st, r = env.http(method, path, body, token=token)
    CORPUS.append(json.dumps(r) if not isinstance(r, str) else r)
    return st, r


def ubtp(env, token, *messages):
    hs = {"v": "1.0", "id": "h", "type": "handshake",
          "body": {"vae": "acme", "versions": ["1.0"], "credentials": {"kind": "bearer", "token": token}}}
    st, r = call(env, "POST", "/ubtp/v1", [hs] + list(messages))
    return r.get("responses", [])[1:] if isinstance(r, dict) else []


def scenario(env, canaries):
    """The security scenario set; returns the administrator's token."""
    # the administrator sets the canary password through the invitation
    path = env.invitation.split(env.url)[1]
    call(env, "POST", path, {"password": canaries["admin_password"]})
    st, r = call(env, "POST", "/api/v1/identity/sign-in", {"username": "admin", "password": canaries["admin_password"]})
    check("scenario: the administrator signs in", st == 200, r)
    admin = r.get("token")
    first_session = r.get("session_id")
    canaries["admin_token"] = admin
    st, me = call(env, "GET", "/api/v1/identity/me", token=admin)
    did = me["principal"]["did"]
    # a user with a password; failures, lockout
    st, p = call(env, "POST", "/api/v1/identity/principals", {"kind": "human", "name": "hana", "role": "member"}, admin)
    hana = p.get("did") if isinstance(p, dict) else None
    call(env, "POST", f"/api/v1/identity/principals/{hana}/password", {"username": "hana", "password": canaries["user_password"]}, admin)
    # a second factor for the administrator: enrol, confirm, a sign-in without the code
    st, t = call(env, "POST", "/api/v1/identity/credentials/admin/totp", {}, admin)
    secret = t.get("secret", "") if isinstance(t, dict) else ""
    canaries["totp_secret"] = secret
    call(env, "POST", "/api/v1/identity/credentials/admin/totp/confirm", {"code": totp(secret)}, admin)
    st, r = call(env, "POST", "/api/v1/identity/sign-in",
                 {"username": "admin", "password": canaries["admin_password"], "totp": totp(secret)})
    second = r.get("token") if isinstance(r, dict) else None
    check("scenario: a sign-in with the second factor", st == 200 and second, r)
    canaries["second_token"] = second or ""
    # sessions: the session policy, a revoked session
    call(env, "PUT", "/api/v1/identity/session-policy", {"kind": "human", "idle_secs": 3600}, admin)
    st, rv = call(env, "DELETE", f"/api/v1/identity/sessions/{first_session}", token=second)
    check("scenario: the first session revoked", st == 200, rv)
    # keys: create, sign, rotate
    call(env, "GET", f"/api/v1/identity/principals/{did}/keys", token=second)
    call(env, "POST", f"/api/v1/identity/principals/{did}/sign", {"message": "aGVsbG8"}, second)
    call(env, "POST", f"/api/v1/identity/principals/{did}/keys/rotate", {}, second)
    # API keys: issued, used, rejected, revoked
    st, k = call(env, "POST", f"/api/v1/identity/principals/{did}/api-keys", {"name": "ci", "scopes": ["read"]}, second)
    key = k.get("key", "") if isinstance(k, dict) else ""
    canaries["api_key"] = key
    call(env, "GET", "/api/v1/identity/me", token=key)
    call(env, "GET", "/api/v1/identity/me", token=key[:-4] + "AAAA")
    call(env, "DELETE", f"/api/v1/identity/api-keys/{k.get('key_id')}", token=second)
    # a disabled principal
    call(env, "POST", f"/api/v1/identity/principals/{hana}/state", {"state": "disabled"}, second)
    # secrets
    call(env, "PUT", "/api/v1/identity/secrets/bank.token", {"value": canaries["secret_value"]}, second)
    call(env, "PUT", "/api/v1/identity/secrets/mail.password", {"value": canaries["secret_value2"]}, second)
    call(env, "DELETE", "/api/v1/identity/secrets/mail.password", token=second)
    # configuration applied and rejected, tenant settings
    doc = open(env.cfg).read()
    call(env, "POST", "/api/v1/admin/config", {"document": doc.replace('level = "debug"', 'level = "debug"\n')}, second)
    call(env, "POST", "/api/v1/admin/config", {"document": doc + "\n[nope]\nx = 1\n"}, second)
    call(env, "PUT", "/api/v1/admin/tenant", {"plan": "standard"}, second)
    # a policy on a branch, and a request it denies
    pid = "01890a5d-ac96-7019-8000-000000000001"
    frames = ubtp(env, second,
         {"v": "1.0", "id": "b", "type": "mutate", "body": {"vae": "acme", "create_branch": {"name": "feature/sec", "from": "main"}}},
         {"v": "1.0", "id": "c", "type": "mutate", "body": {"vae": "acme", "branch": "feature/sec", "commit": {"changes": [
             {"op": "create", "class": "Person", "object_id": "01890a5d-ac96-7019-8000-000000000002", "slug": "p1",
              "body": {"name": "Person one"}},
             {"op": "create", "class": "Policy", "object_id": pid, "slug": "people-only",
              "body": {"name": "people-only", "description": "hygiene suite",
                       "source": "permit(principal, action, resource) when { resource is UBOS::Model::Organisation };"}}]}}},
         {"v": "1.0", "id": "r", "type": "query", "body": {"vae": "acme", "branch": "feature/sec", "read": "ubos://acme/Person/p1"}},
         {"v": "1.0", "id": "w", "type": "mutate", "body": {"vae": "acme", "branch": "feature/sec", "commit": {"changes": [
             {"op": "create", "class": "Person", "object_id": "01890a5d-ac96-7019-8000-000000000003", "slug": "p2",
              "body": {"name": "Person two"}}]}}})
    denied = [f for f in frames if f.get("reply_to") in ("r", "w")]
    check("scenario: the policy refuses the read and the commit of a Person",
          len(denied) == 2 and all(f.get("type") == "error" for f in denied), frames)
    # a UBTP workload with failures: unknown objects, a bad commit, a wrong token
    ubtp(env, second,
         {"v": "1.0", "id": "q1", "type": "query", "body": {"vae": "acme", "read": "ubos://acme/Person/none"}},
         {"v": "1.0", "id": "m1", "type": "mutate", "body": {"vae": "acme", "commit": {"changes": [
             {"op": "create", "class": "Person", "object_id": "bad", "slug": "x", "body": {}}]}}})
    ubtp(env, canaries["wrong_password"])
    # the diagnostic bundle
    st, b = call(env, "POST", "/api/v1/admin/diagnostics", {}, second)
    check("scenario: a diagnostic bundle", st == 200, str(b)[:300])
    # failed sign-ins last, since they slow the source down: a wrong password until the
    # account locks, an unknown user, the administrator without the second factor
    st, r = call(env, "POST", "/api/v1/identity/sign-in", {"username": "admin", "password": canaries["admin_password"]})
    check("scenario: a sign-in without the second factor is refused", st >= 400, r)
    for _ in range(6):
        call(env, "POST", "/api/v1/identity/sign-in", {"username": "hana", "password": canaries["wrong_password"]})
    call(env, "POST", "/api/v1/identity/sign-in", {"username": "nobody", "password": canaries["wrong_password"]})
    return second


def scan(env, canaries, token):
    time.sleep(2)  # the node mirrors the kernel's security log once a second
    sources = {}
    sources["node log (logs and spans)"] = open(os.path.join(env.work, "server.log"), errors="replace").read()
    st, spans = env.http("GET", "/api/v1/admin/telemetry/spans?limit=2000", token=token)
    sources["telemetry span buffer"] = json.dumps(spans)
    st, bundle = env.http("POST", "/api/v1/admin/diagnostics", {}, token=token)
    sources["diagnostic bundle"] = json.dumps(bundle)
    st, export = env.http("GET", "/api/v1/identity/security-log/export", token=token)
    sources["security-log export"] = json.dumps(export)
    sources["client responses"] = "\n".join(CORPUS)
    found = []
    for name, value in canaries.items():
        if not value or len(value) < 8:
            continue
        for where, text in sources.items():
            # a token may be returned to the client it was issued to, and an API key once
            # to its creator: only those responses are exempt
            if where == "client responses" and name in ("admin_token", "second_token", "api_key", "totp_secret"):
                continue
            if value in text:
                found.append(f"{name} in {where}")
    sizes = {k: len(v) for k, v in sources.items()}
    check(f"NR-SEC-009: 0 canary matches over {len(sources)} sources ({sum(sizes.values())} bytes)", not found, found)
    return sizes, export


def coverage(env, token):
    st, log = env.http("GET", "/api/v1/identity/security-log?limit=10000", token=token)
    items = log.get("entries", []) if isinstance(log, dict) else []
    kinds = set()
    for e in items:
        k = e.get("kind")
        kinds.add(k)
        # the kernel's security log mirrored into the tenant's (DSN-NOD-111)
        inner = (e.get("record") or {}).get("kind")
        if k == "dvm.security" and inner:
            kinds.add(f"{k}:{inner}")
    missing = []
    for cat, ks in EXPECTED.items():
        for k in ks:
            if k not in kinds:
                missing.append(f"{cat}: {k}")
    total = sum(len(v) for v in EXPECTED.values())
    check(f"NR-SEC-010: {total - len(missing)} of {total} PH-1 security event types of FR-AUD-012 present",
          not missing, {"missing": missing, "seen": sorted(x for x in kinds if x)})
    st, v = env.http("GET", "/api/v1/identity/security-log/verify", token=token)
    check("FR-AUD-012: the security log's hash chain verifies", st == 200 and isinstance(v, dict) and v.get("ok") is True, v)
    return sorted(x for x in kinds if x)


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--binary", required=True)
    ap.add_argument("--work", default=None)
    ap.add_argument("--report", default=None)
    ap.add_argument("--pg-bin", default=None)
    ap.add_argument("--keep", action="store_true")
    a = ap.parse_args()
    a.package, a.points, a.spacing = None, 0, 0
    a.work = a.work or (f"/var/tmp/ubos-hyg-{os.getpid()}" if ops.ROOT else os.path.join(os.getcwd(), f"ubos-hyg-{os.getpid()}"))
    if os.path.exists(a.work):
        sys.exit(f"{a.work} exists")
    os.makedirs(a.work)
    env = ops.Env(a)
    started = time.time()
    node = None
    steps = {}
    try:
        ops.install(env)
        text = open(env.cfg).read()
        text = text.replace('level = "info"', 'level = "debug"')
        open(env.cfg, "w").write(text)
        if not ops.postgres(env):
            raise SystemExit
        etc = os.path.dirname(env.cfg)
        canaries = {
            "admin_password": "CanaryAdmin-" + secrets.token_hex(8),
            "user_password": "CanaryUser-" + secrets.token_hex(8),
            "wrong_password": "CanaryWrong-" + secrets.token_hex(8),
            "secret_value": "CanarySecret-" + secrets.token_hex(12),
            "secret_value2": "CanarySecret2-" + secrets.token_hex(12),
            "db_password": env.db_password,
            "root_key": open(os.path.join(etc, "root.key")).read().strip(),
            "backup_key": open(os.path.join(etc, "backup.key")).read().strip(),
        }
        r = env.run(env.bin, "bootstrap", "--config", env.cfg)
        import re
        m = re.search(r"(http://\S+/api/v1/identity/invitations/\S+)", r.stdout)
        env.invitation = m.group(1) if m else None
        check("setup: bootstrap", r.returncode == 0 and m, r.stdout[-500:] + r.stderr[-500:])
        canaries["invitation"] = m.group(1).rsplit("/", 1)[1] if m else ""
        node = ops.start_node(env)
        token = scenario(env, canaries)
        sizes, _ = scan(env, canaries, token)
        kinds = coverage(env, token)
        steps = {"corpus_bytes": sizes, "event_kinds": kinds, "canaries": sorted(canaries)}
    finally:
        if node:
            ops.stop_node(node)
        if getattr(env, "pg_data", None):
            env.run(os.path.join(env.pg_bin, "pg_ctl"), "-D", env.pg_data, "-m", "fast", "-w", "stop", pg=True)
        failed = [c for c in ops.CHECKS if not c["ok"]]
        report = {"suite": "SUITE-NOD-AUTH", "items": ["VER-SEC-2001"], "part": "PH-1 step 19a", "decision": "DEC-085",
                  "pass": not failed and bool(ops.CHECKS), "version": getattr(env, "version", None),
                  "host": {"system": platform.platform(), "machine": platform.machine(), "cpus": os.cpu_count()},
                  "expected": EXPECTED, "seconds": round(time.time() - started, 1), "steps": steps, "checks": ops.CHECKS}
        if a.report:
            os.makedirs(os.path.dirname(os.path.abspath(a.report)), exist_ok=True)
            json.dump(report, open(a.report, "w"), indent=2)
        print(f"\nVER-SEC-2001: {len(ops.CHECKS) - len(failed)} of {len(ops.CHECKS)} checks passed ({report['seconds']} s)")
        if not a.keep:
            shutil.rmtree(a.work, ignore_errors=True)
    sys.exit(1 if failed or not ops.CHECKS else 0)


if __name__ == "__main__":
    main()
