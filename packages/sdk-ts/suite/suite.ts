// The TypeScript runner of SUITE-SDK-PROTO and SUITE-SDK-NET (PH-1 step 17d, DEC-078): every
// case of verify/sdk/cases.json against a node behind the fault proxy of
// verify/sdk/sdk_suite.py, which starts it:
//
//   node build/suite/suite.js --url PROXY --direct NODE --control CONTROL --vae V --user U
//        --password P --api-key K --token T --binding ws|http --tag TAG [--cases P01,N03]
//
// One JSON line per case: {"case", "ok", "checks": [{"name", "ok", "detail"}], "ms"}. The
// typed API is the one `forge types` generated for verify/sdk/fixture (suite/generated).

import {
  type Auth,
  type Binding,
  type CallOptions,
  type Client,
  connect,
  type ConnectOptions,
  Decimal,
  IamNotFound,
  Item,
  LogicFloatToDecimal,
  Money,
  MutateResult,
  type Page,
  QryInvalidQuery,
  REGISTRY,
  RuleValidationFailed,
  type Subscription,
  TraceContext,
  TransportError,
  TxnHeadMoved,
  TxnIdempotencyMismatch,
  UbosError,
  type Update,
  errorOf,
  rejectFloats,
  uuid7,
  withRetry,
} from "../src/index.js";
import { Account, Customer, Party, type PartyItem } from "./generated/types.js";

type Json = Record<string, unknown>;

const args = new Map<string, string>();
const argv = process.argv.slice(2);
for (let i = 0; i + 1 < argv.length; i += 2) args.set(argv[i].replace(/^--/, ""), argv[i + 1]);
const arg = (k: string) => args.get(k) ?? "";
const vae = arg("vae");
const tag = arg("tag");
const binding: Binding = arg("binding") === "http" ? "http" : "ws";
const password: Auth = { kind: "password", username: arg("user"), password: arg("password") };
const BR = "feature/sdk";

class Case {
  checks: { name: string; ok: boolean; detail: string }[] = [];
  check(name: string, ok: boolean, detail?: unknown): void {
    let d = "";
    if (!ok) {
      d = detail instanceof Error ? `${detail.name}: ${detail.message} ${JSON.stringify((detail as UbosError).details ?? null)}` : JSON.stringify(detail, (_, v) => (v instanceof Item || v instanceof MutateResult ? v.raw : v)) ?? String(detail);
    }
    this.checks.push({ name, ok, detail: d.slice(0, 600) });
  }
  get ok(): boolean {
    return this.checks.length > 0 && this.checks.every((c) => c.ok);
  }
}

const opts = (url: string, auth: Auth, more: Partial<ConnectOptions> = {}): ConnectOptions => ({ url, vae, auth, transport: binding, ...more });
let next = 0;
const oid = (): [string, string] => [uuid7(), `${tag}-${next++}`];
const uri = (cls: string, slug: string) => `ubos://${vae}/${cls}/${slug}`;
const attempt = async <T>(p: Promise<T>): Promise<T | Error> => p.catch((e: unknown) => (e instanceof Error ? e : new Error(String(e))));
const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));

/** A control request to the fault proxy. */
async function control(path: string): Promise<Json> {
  try {
    const r = await fetch(`${arg("control")}${path}`, { method: "POST" });
    return (await r.json()) as Json;
  } catch {
    return {};
  }
}

async function customer(c: Client, name: string, extra: Json = {}): Promise<[string, string]> {
  const [id, slug] = oid();
  await c.commit([{ op: "create", class: "Customer", object_id: id, slug, body: { name, country: tag, ...extra } }]);
  return [id, slug];
}

async function rename(c: Client, id: string, slug: string, name: string, o?: CallOptions): Promise<MutateResult> {
  const it = await c.get(uri("Customer", slug));
  return c.commit([{ op: "update", object_id: id, base: it.version, body: { name } }], o);
}

async function account(c: Client): Promise<[string, string]> {
  const [id, slug] = oid();
  await c.commit([{ op: "create", class: "Account", object_id: id, slug, body: { name: "acct", balance: "0" } }], { branch: BR });
  return [id, slug];
}

const historyLen = (v: Json) => ((v.items as unknown[] | undefined) ?? []).length;

function data(n: number, seed: number): Uint8Array<ArrayBuffer> {
  const d = new Uint8Array(n);
  for (let i = 0; i < n; i++) d[i] = (Math.imul((i ^ seed) >>> 0, 2654435761) >>> 13) & 0xff;
  return d;
}

/** A canonical instant: six fraction digits. */
const instant = (d: Date) => d.toISOString().replace(/\.(\d{3})Z$/, ".$1000Z");

const equal = (a: Uint8Array, b: Uint8Array) => a.length === b.length && a.every((x, i) => x === b[i]);

async function untilEmit(sub: Subscription, pred: (b: Json) => boolean, ms: number): Promise<boolean> {
  const until = Date.now() + ms;
  while (Date.now() < until) {
    const u = await sub.next(Math.max(1, until - Date.now()));
    if (!u) return false;
    if (u.kind === "change" && pred(u.body)) return true;
  }
  return false;
}

async function run(c: Client, direct: Client, id: string): Promise<Case> {
  const k = new Case();
  switch (id) {
    case "P01": {
      k.check("protocol 1.0 negotiated", c.version === "1.0", c.session);
      k.check("the session names its principal", c.session.principal != null, c.session);
      break;
    }
    case "P02": {
      for (const [name, auth] of [
        ["password", password],
        ["api key", { kind: "api_key", key: arg("api-key") }],
        ["bearer token", { kind: "token", token: arg("token") }],
      ] as [string, Auth][]) {
        const r = await attempt(connect(opts(arg("url"), auth)).then(async (x) => (await x.queryPage("from Customer", undefined, 1), x.close())));
        k.check(`${name}: connected and read`, !(r instanceof Error), r);
      }
      const t0 = Date.now();
      const bad = await attempt(connect(opts(arg("url"), { ...password, password: "wrong" } as Auth)));
      k.check("a wrong password: a typed IAM error, not retryable, not retried", bad instanceof UbosError && bad.area === "IAM" && !bad.retryable && Date.now() - t0 < 5000, bad);
      break;
    }
    case "P03": {
      const changes = [0, 1, 2].map((i) => {
        const [id2, slug] = oid();
        return { op: "create", class: "Customer", object_id: id2, slug, body: { name: `p03 ${i}`, country: tag } };
      });
      const r = await attempt(c.commit(changes));
      k.check("the process, one commit and three versions", r instanceof MutateResult && !!r.processId && r.commits.length === 1 && Object.keys(r.versions).length === 3, r);
      break;
    }
    case "P04": {
      const [id2, slug] = oid();
      const created = await c.commit([{ op: "create", class: "Customer", object_id: id2, slug, body: { name: "p04 first", country: tag } }]);
      const firstCommit = created.commitId;
      const u = uri("Customer", slug);
      const between = instant(new Date());
      await sleep(1100);
      const upd = await attempt(rename(c, id2, slug, "p04 second"));
      k.check("update", !(upd instanceof Error), upd);
      const commit = upd instanceof MutateResult ? upd.commitId : undefined;
      const main = await attempt(c.get<Json>(u, { branch: "main" }));
      k.check("branch main: the latest version with its version and evidence", main instanceof Item && main.body.name === "p04 second" && main.version.startsWith("sha256:") && Object.keys(main.evidence).length > 0, main);
      const atTime = await attempt(c.get<Json>(u, { time: between }));
      k.check("time: the snapshot before the update", atTime instanceof Item && atTime.body.name === "p04 first", atTime);
      const at = await attempt(c.get<Json>(u, { commit }));
      k.check("commit: the version of that commit", at instanceof Item && at.body.name === "p04 second", at);
      const old = await attempt(c.get<Json>(u, { commit: firstCommit }));
      k.check("commit: the first version at its commit", old instanceof Item && old.body.name === "p04 first", old);
      const today = between.slice(0, 10);
      const both = await attempt(c.get<Json>(u, { asof: today, time: between }));
      k.check(
        "time and asof: the evidence names the snapshot's commit and the valid segment",
        both instanceof Item && Object.values(both.evidence).includes(firstCommit) && String((both.raw.valid as Json | undefined)?.from ?? "z") <= today,
        both,
      );
      const missing = await attempt(c.get(uri("Customer", `${tag}-missing`)));
      k.check("a missing object: IamNotFound", missing instanceof IamNotFound, missing);
      break;
    }
    case "P05": {
      const q = `from Customer where this.country == "${tag}-p05" order by this.name`;
      const changes = Array.from({ length: 25 }, (_, i) => {
        const [id2, slug] = oid();
        return { op: "create", class: "Customer", object_id: id2, slug, body: { name: `p05 ${String(i).padStart(2, "0")}`, country: `${tag}-p05` } };
      });
      await c.commit(changes);
      const p1 = await attempt(c.queryPage(q, undefined, 10));
      if (p1 instanceof Error) {
        k.check("first page", false, p1);
        break;
      }
      const [id2, slug] = oid();
      await c.commit([{ op: "create", class: "Customer", object_id: id2, slug, body: { name: "p05 extra", country: `${tag}-p05` } }]);
      const items: Item[] = [...p1.items];
      const snaps = [JSON.stringify(p1.evaluatedAt)];
      let pages = 1;
      for (let p: Page | undefined = await p1.next(); p; p = await p.next()) {
        items.push(...p.items);
        snaps.push(JSON.stringify(p.evaluatedAt));
        pages++;
      }
      k.check("three pages of 10, 10 and 5 items", pages === 3 && items.length === 25, [pages, items.length]);
      k.check("every page of the first page's snapshot", new Set(snaps).size === 1, snaps);
      const names = items.map((i) => String((i.body as Json).name));
      k.check("ordered, without duplicates", JSON.stringify(names) === JSON.stringify([...names].sort()) && new Set(names).size === names.length, names);
      let fresh = 0;
      for await (const x of c.query(q, { pageSize: 10 })) fresh += x ? 1 : 0;
      k.check("a new query sees the commit made between pages", fresh === 26, fresh);
      break;
    }
    case "P06": {
      const [id2, slug] = oid();
      const created = await c.commit([{ op: "create", class: "Customer", object_id: id2, slug, body: { name: "p06 a", country: tag } }]);
      const u = uri("Customer", slug);
      const upd = await rename(c, id2, slug, "p06 b");
      const h = await attempt(c.history(u));
      k.check("history: the two commits of the object", !(h instanceof Error) && historyLen(h) === 2, h);
      const t = await attempt(c.timeline(u));
      k.check("timeline: the valid-time segments", !(t instanceof Error) && historyLen(t) > 0, t);
      const d = await attempt(c.diff({ cseq: created.cseq ?? 0 }, { cseq: upd.cseq ?? 0 }));
      k.check("diff: the change between the two commits names the object", !(d instanceof Error) && JSON.stringify(d).includes(id2), d);
      const [, aslug] = await account(c);
      const x = await attempt(c.explain({ target: uri("Account", aslug), kind: "decision", port: "close" }, { branch: BR }));
      k.check("explain: the decision of a port", !(x instanceof Error), x);
      break;
    }
    case "P07": {
      const [, slug] = await account(c);
      const target = uri("Account", slug);
      const r = await attempt(c.invoke(target, "mark", { tag: "p07" }, { branch: BR }));
      k.check("the port's result", r instanceof MutateResult && r.result === "p07" && !!r.processId, r);
      const item = await attempt(c.get<Json>(target, { branch: BR }));
      k.check("its effect on the branch", item instanceof Item && item.body.tag === "p07", item);
      break;
    }
    case "P08": {
      const [id2, slug] = await customer(c, "p08");
      const u = uri("Customer", slug);
      const o: CallOptions = { idempotencyKey: uuid7(), trace: new TraceContext() };
      const v = (await c.get(u)).version;
      const ch = [{ op: "update", object_id: id2, base: v, body: { name: "p08 renamed" } }];
      const r1 = await attempt(c.commit(ch, o));
      const r2 = await attempt(c.commit(ch, o));
      const other = await connect(opts(arg("url"), password));
      const r4 = await attempt(other.commit(ch, o));
      other.close();
      const same = (a: unknown, b: unknown) => a instanceof MutateResult && b instanceof MutateResult && !!a.processId && a.processId === b.processId && a.commitId === b.commitId;
      k.check("the same key returns the first process", same(r1, r2), [r1, r2]);
      k.check("in another session too", same(r1, r4), [r1, r4]);
      const h = await c.history(u);
      k.check("one commit for the three calls", historyLen(h) === 2, h);
      const mm = await attempt(c.commit([{ op: "update", object_id: id2, base: v, body: { name: "p08 other" } }], o));
      k.check("another payload under the key: TxnIdempotencyMismatch", mm instanceof TxnIdempotencyMismatch, mm);
      const r3 = await attempt(rename(c, id2, slug, "p08 again"));
      k.check("a new key runs again", r3 instanceof MutateResult && r1 instanceof MutateResult && r3.processId !== r1.processId, r3);
      break;
    }
    case "P09": {
      const [id2, slug] = await customer(c, "p09");
      const u = uri("Customer", slug);
      const before = historyLen(await c.history(u));
      const v = (await c.get(u)).version;
      const s0 = await control("/stats");
      await control("/fault?mode=cut_reply");
      const r = await attempt(c.commit([{ op: "update", object_id: id2, base: v, body: { name: "p09 once" } }]));
      const s1 = await control("/stats");
      k.check("the reply was cut by the proxy", Number(s1.cuts) > Number(s0.cuts), [s0, s1]);
      k.check("the retried commit returns the first result", r instanceof MutateResult, r);
      const after = historyLen(await c.history(u));
      k.check("committed once", after === before + 1, [before, after]);
      const head = r instanceof MutateResult ? (r.cseq ?? 1) : 1;
      const t0 = Date.now();
      const stale = await attempt(c.commit([{ op: "update", object_id: id2, body: { name: "p09 stale" } }], { base: head - 1 }));
      k.check("a stale base: TxnHeadMoved, not retried", stale instanceof TxnHeadMoved && Date.now() - t0 < 3000, stale);
      break;
    }
    case "P10": {
      const p = { maxAttempts: 3, baseMs: 1, capMs: 1, deadlineMs: 5000 };
      const wrong: [string, number][] = [];
      for (const [code, [retryable]] of Object.entries(REGISTRY)) {
        let n = 0;
        await withRetry(p, Date.now() + 5000, false, () => {
          n++;
          return Promise.reject(errorOf(code, ""));
        }).catch(() => undefined);
        const expect = code === "TXN.HEAD_MOVED" ? 1 : retryable ? 3 : 1;
        if (n !== expect) wrong.push([code, n]);
      }
      k.check(`${Object.keys(REGISTRY).length} registered codes: retried exactly when retryable`, wrong.length === 0, wrong);
      let n = 0;
      await withRetry(p, Date.now() + 5000, true, () => (n++, Promise.reject(errorOf("TXN.HEAD_MOVED", "")))).catch(() => undefined);
      k.check("TXN.HEAD_MOVED retried when the call has no base", n === 3, n);
      n = 0;
      await withRetry(p, Date.now() + 5000, false, () => (n++, Promise.reject(new TransportError("reset")))).catch(() => undefined);
      k.check("transport failures retried", n === 3, n);
      break;
    }
    case "P11": {
      const status = (code: string) => REGISTRY[code as keyof typeof REGISTRY]?.[2];
      const expect = (name: string, r: unknown, cls: typeof UbosError) =>
        k.check(name, r instanceof cls && r.area !== undefined && r.status === status(r.code) && !r.retryable, r);
      expect("a missing object: IamNotFound (404)", await attempt(c.get(uri("Customer", "no-such-object"))), IamNotFound);
      expect("a malformed query: QryInvalidQuery (400)", await attempt(c.queryPage("from Customer where", undefined, 1)), QryInvalidQuery);
      let [id2, slug] = oid();
      const v = await attempt(c.commit([{ op: "create", class: "Customer", object_id: id2, slug, body: { name: "p11", rating: 9 } }]));
      expect("a failed validation: RuleValidationFailed (422)", v, RuleValidationFailed);
      k.check("its details name the validation", v instanceof UbosError && JSON.stringify(v.details).includes("rating_range"), v);
      [id2, slug] = oid();
      const u = await attempt(c.commit([{ op: "create", class: "NoSuchClass", object_id: id2, slug, body: {} }]));
      k.check("an unknown class: a registered error", u instanceof UbosError && u.status !== undefined, u);
      const f = await attempt(c.commit([{ op: "create", class: "Customer", object_id: id2, slug, body: { name: "x", credit_limit: { amount: 1.5, currency: "USD" } } }]));
      k.check("a float: LogicFloatToDecimal at the boundary with its path", f instanceof LogicFloatToDecimal && (f.details as Json).path === "commit.changes.0.body.credit_limit.amount", f);
      break;
    }
    case "P12": {
      const [id2, slug] = await customer(c, "p12");
      const sub = await c.subscribe(uri("Customer", slug));
      const qsub = await c.subscribe({ query: { from: "Customer", where: `this.country == "${tag}-p12q"` } });
      const esub = await c.subscribe({ events: ["AccountClosed"] }, undefined, { branch: BR });
      await rename(c, id2, slug, "p12 renamed");
      k.check("object: the change emitted", await untilEmit(sub, (b) => JSON.stringify(b).includes(id2), 10000));
      const [qid] = await customer(c, "p12 q", { country: `${tag}-p12q` });
      k.check("query: a new matching object emitted", await untilEmit(qsub, (b) => JSON.stringify(b).includes(qid), 10000), qid);
      const [, aslug] = await account(c);
      await c.invoke(uri("Account", aslug), "close", {}, { branch: BR });
      k.check("events: the released AccountClosed emitted", await untilEmit(esub, (b) => JSON.stringify(b).includes("AccountClosed"), 10000));
      // a handler instead of iteration
      const seen: Update[] = [];
      const hsub = await c.subscribe(uri("Customer", slug), (u) => seen.push(u));
      await rename(c, id2, slug, "p12 handler");
      for (let i = 0; i < 50 && seen.length === 0; i++) await sleep(100);
      k.check("a handler receives the update", seen.some((u) => u.kind === "change"), seen);
      k.check("the cursor of the last update", typeof sub.cursor === "string", sub.cursor);
      for (const [name, s] of [["object", sub], ["query", qsub], ["events", esub], ["handler", hsub]] as [string, Subscription][]) {
        const r = await attempt(s.close());
        k.check(`${name}: unsubscribed`, !(r instanceof Error), r);
      }
      break;
    }
    case "P13": {
      const d = data(1_200_000, 13);
      let location = "";
      const s0 = await control("/stats");
      await control("/fault?mode=cut_after&bytes=500000");
      const once = await connect(opts(arg("url"), password, { retry: { maxAttempts: 1 } }));
      const first = await attempt(once.files.upload(d, { name: "p13.bin", chunkSize: 256 * 1024, onLocation: (l) => (location = l) }));
      once.close();
      await control("/fault?mode=none");
      const s1 = await control("/stats");
      k.check("the first attempt interrupted by a dropped connection", first instanceof Error && Number(s1.cuts) > Number(s0.cuts), [first, s1]);
      const offset = await attempt(c.files.offset(location));
      k.check("the node kept a part", typeof offset === "number" && offset > 0 && offset < d.length, [location, offset]);
      let seen = 0;
      const up = await attempt(c.files.upload(new Blob([d]), { resume: location, chunkSize: 256 * 1024, onProgress: (n) => (seen = n) }));
      k.check("resumed from its location, completed and verified by SHA-256", !(up instanceof Error) && up.reference.size === d.length && seen === d.length, up);
      if (!(up instanceof Error)) {
        const [id2, slug] = oid();
        const r = await attempt(c.commit([{ op: "create", class: "Document", object_id: id2, slug, body: { title: "p13", passport: up.reference } }]));
        k.check("referred to by a blob field", !(r instanceof Error), r);
        const doc = uri("Document", slug);
        const whole = await attempt(c.files.download(doc, "passport"));
        k.check("the whole file downloaded and verified", whole instanceof Uint8Array && equal(whole, d), whole);
        const part = await attempt(c.files.download(doc, "passport", { range: [100_000, 300_000] }));
        k.check("a range", part instanceof Uint8Array && equal(part, d.subarray(100_000, 300_000)), part);
      }
      break;
    }
    case "P14": {
      const big = "12345678901234567890.1234567891";
      const [id2, slug] = oid();
      const r = await attempt(c.commit([{ op: "create", class: "Account", object_id: id2, slug, body: { name: "p14", balance: new Decimal(big) } }]));
      k.check("a 30-digit decimal committed", !(r instanceof Error), r);
      const back = await attempt(c.get<Json>(uri("Account", slug)));
      k.check("read back exactly", back instanceof Item && back.body.balance === big, back);
      k.check("money in canonical form", JSON.stringify(new Money("1250.50", "USD")) === '{"amount":"1250.5","currency":"USD"}');
      k.check("negative zero is zero", new Decimal("-0.000").toString() === "0");
      k.check("an exponent is refused", (() => { try { new Decimal("1e3"); return false; } catch { return true; } })());
      let f: unknown;
      try {
        rejectFloats({ a: [1, { b: 0.1 }] });
      } catch (e) {
        f = e;
      }
      k.check("a float refused with its path", f instanceof LogicFloatToDecimal && (f.details as Json).path === "a.1.b", f);
      break;
    }
    case "P15": {
      const [, cslug] = await customer(c, "p15 customer", { credit_limit: new Money("10", "USD") });
      const [sid, sslug] = oid();
      await c.commit([{ op: "create", class: "Supplier", object_id: sid, slug: sslug, body: { name: "p15 supplier", country: tag, iban: "DE00" } }]);
      const got = await attempt(Customer.get(c, cslug));
      k.check("a typed read with its money", !(got instanceof Error) && got.class === "Customer" && new Decimal(got.body.credit_limit?.amount ?? "0").equals("10"), got);
      const q = Party.query().filter("country", "==", tag).orderBy("name");
      k.check("the builder compiles to DAT-Query", JSON.stringify(q.toQuery()) === JSON.stringify({ from: "Party", where: `this.country == "${tag}"`, order_by: [{ path: "this.name", dir: "asc" }] }), q.toQuery());
      const all: PartyItem[] = await q.all(c);
      const text: string[] = [];
      for await (const x of c.query(`from Party where this.country == "${tag}" order by this.name`)) text.push(x.objectId);
      k.check("the same results as the text query", all.length > 0 && JSON.stringify(all.map((x) => x.objectId)) === JSON.stringify(text), [all.map((x) => x.objectId), text]);
      const classes = all.map((x) => x.class);
      k.check("polymorphic results narrowed by class", classes.includes("Customer") && classes.includes("Supplier"), classes);
      // narrowing on `class` gives each variant its own fields in the type checker
      const ibans = all.map((x) => (x.class === "Supplier" ? x.body.iban : undefined)).filter(Boolean);
      k.check("a variant's own fields", ibans[0] === "DE00", ibans);
      const [, aslug] = await account(c);
      const r = await attempt(Account.ports.rename(c, uri("Account", aslug), { name: "p15 renamed" }, { branch: BR }));
      k.check("a typed port call", r instanceof MutateResult && r.result === "p15 renamed", r);
      break;
    }
    // ---- SUITE-SDK-NET ----
    case "N01": {
      const r0 = await attempt(c.queryPage("from Customer", undefined, 1));
      k.check("before", !(r0 instanceof Error), r0);
      const s = await control("/fault?mode=drop");
      k.check("the proxy dropped the connections", "mode" in s, s);
      const r1 = await attempt(c.queryPage("from Customer", undefined, 1));
      k.check("the next request reconnects", !(r1 instanceof Error), r1);
      break;
    }
    case "N02": {
      const s0 = await control("/stats");
      await control("/fault?mode=cut_reply");
      const r = await attempt(c.queryPage("from Customer", undefined, 5));
      const s1 = await control("/stats");
      k.check("the reply was cut", Number(s1.cuts) > Number(s0.cuts), [s0, s1]);
      k.check("the read retried and answered", !(r instanceof Error), r);
      break;
    }
    case "N03": {
      await control("/fault?mode=latency&ms=300");
      const t0 = Date.now();
      const r = await attempt(c.queryPage("from Customer", undefined, 1));
      const readMs = Date.now() - t0;
      const w = await attempt(customer(c, "n03"));
      await control("/fault?mode=none");
      k.check("a read completes", !(r instanceof Error), r);
      k.check("a commit completes", !(w instanceof Error), w);
      k.check("the latency was applied", readMs >= 550, readMs);
      break;
    }
    case "N04": {
      await control("/fault?mode=partition&ms=3000");
      const t0 = Date.now();
      const r = await attempt(c.queryPage("from Customer", undefined, 1));
      const ms = Date.now() - t0;
      k.check("the request completed after the partition healed", !(r instanceof Error) && ms >= 2500, [r, ms]);
      break;
    }
    case "N05": {
      const short = await connect(opts(arg("url"), password, { requestTimeoutMs: 1500, retry: { deadlineMs: 4000 } }));
      await control("/fault?mode=partition&ms=9000");
      const t0 = Date.now();
      const r = await attempt(short.queryPage("from Customer", undefined, 1));
      const ms = Date.now() - t0;
      await control("/fault?mode=none");
      short.close();
      k.check("a transport error, retryable", r instanceof TransportError && r.retryable, r);
      k.check("within the deadline and one request timeout", ms < 6000, ms);
      const again = await attempt(c.queryPage("from Customer", undefined, 1));
      k.check("served again after the partition", !(again instanceof Error), again);
      break;
    }
    case "N06": {
      const [id2, slug] = await customer(c, "n06 0");
      const sub = await c.subscribe(uri("Customer", slug));
      const want = new Set<number>();
      for (let i = 1; i <= 9; i++) {
        const r = await attempt(rename(direct, id2, slug, `n06 ${i}`));
        if (r instanceof MutateResult && r.cseq !== undefined) want.add(r.cseq);
        else k.check(`update ${i}`, false, r);
        // the subscriber makes no request: the SDK notices the dropped stream, reconnects
        // and resubscribes from its cursor by itself
        if (i % 3 === 0) await control("/fault?mode=drop");
        await sleep(150);
      }
      const seen = new Set<number>();
      const until = Date.now() + 20000;
      while ([...want].some((q) => !seen.has(q)) && Date.now() < until) {
        const u = await sub.next(5000);
        if (u?.kind === "reset") {
          k.check("no reset", false, u.error);
          break;
        }
        for (const ch of ((u?.body.changes as Json[] | undefined) ?? [])) if (ch.object_id === id2) seen.add(Number(ch.cseq));
      }
      k.check("all 9 updates arrived across 3 dropped connections, without a request of the subscriber", want.size === 9 && [...want].every((q) => seen.has(q)), [[...want], [...seen]]);
      await sub.close().catch(() => undefined);
      break;
    }
    case "N07": {
      const [id2, slug] = await customer(c, "n07");
      const sub = await c.subscribe(uri("Customer", slug));
      const s = await control("/restart");
      k.check("the node restarted", s.restarted === true, s);
      const other = await connect(opts(arg("direct"), password));
      const r = await attempt(rename(c, id2, slug, "n07 after"));
      k.check("the client reconnected and signed in again", !(r instanceof Error), r);
      const o = await attempt(rename(other, id2, slug, "n07 other"));
      const q = o instanceof MutateResult ? o.cseq : undefined;
      const got = await untilEmit(sub, (b) => ((b.changes as Json[] | undefined) ?? []).some((ch) => ch.cseq === q && q !== undefined), 15000);
      k.check("the subscription resumed: another client's change arrived", got, q);
      other.close();
      await sub.close().catch(() => undefined);
      break;
    }
    case "N08": {
      const d = data(1_500_000, 8);
      const s0 = await control("/stats");
      await control("/fault?mode=cut_after&bytes=600000");
      const up = await attempt(c.files.upload(d, { chunkSize: 256 * 1024 }));
      await control("/fault?mode=none");
      const s1 = await control("/stats");
      k.check("a connection dropped during the upload", Number(s1.cuts) > Number(s0.cuts), [s0, s1]);
      k.check("the upload resumed and completed, verified by SHA-256", !(up instanceof Error) && up.reference.size === d.length, up);
      break;
    }
    default:
      k.check("known case", false, id);
  }
  return k;
}

const c = await attempt(connect(opts(arg("url"), password)));
const direct = await attempt(connect(opts(arg("direct"), password)));
if (c instanceof Error || direct instanceof Error) {
  console.log(JSON.stringify({ case: "setup", ok: false, checks: [{ name: "connect", ok: false, detail: String(c instanceof Error ? c : direct) }] }));
  process.exit(1);
}
const ids = args.has("cases")
  ? arg("cases").split(",")
  : [...Array.from({ length: 15 }, (_, i) => `P${String(i + 1).padStart(2, "0")}`), ...Array.from({ length: 8 }, (_, i) => `N${String(i + 1).padStart(2, "0")}`)];
let failed = 0;
for (const id of ids) {
  const t0 = Date.now();
  let k: Case;
  try {
    k = await run(c, direct, id);
  } catch (e) {
    k = new Case();
    k.check("the case ran to its end", false, e);
  }
  if (!k.ok) failed++;
  console.log(JSON.stringify({ case: id, ok: k.ok, checks: k.checks, ms: Date.now() - t0 }));
}
c.close();
direct.close();
process.exit(failed ? 1 : 0);
