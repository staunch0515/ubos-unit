// A tour of the SDK against a running node, shared by Node.js (tour.ts) and browsers
// (browser.ts): connection, typed errors, mutations with idempotency keys, exact decimals,
// paged queries, subscriptions that survive a restart of the node, and files.
//
// The VAE needs the classes `Customer` (`name`, `email`, `credit_limit` money) and
// `Document` (`title`, `passport` blob).

import {
  type Binding,
  type Client,
  connect,
  IamDenied,
  Item,
  LogicFloatToDecimal,
  Money,
  TraceContext,
  UbosError,
  uuid7,
} from "../src/index.js";

export interface TourOptions {
  url: string;
  vae: string;
  username: string;
  password: string;
  transport: Binding;
  tenant?: string;
  /** Distinguishes the objects of concurrent tours. */
  seed: number;
  log: (line: string) => void;
  /** When given, called once mid-tour while the node restarts; resolves when it is back. */
  restart?: () => Promise<void>;
}

const oid = (n: number) => `01890a5d-ac96-7001-8000-${(0x7d0000 + n).toString(16).padStart(12, "0")}`;

/** Run the tour; the number of failed checks. */
export async function tour(o: TourOptions): Promise<{ ok: number; failed: number }> {
  let ok = 0;
  let failed = 0;
  const check = (name: string, pass: boolean, detail?: unknown) => {
    if (pass) ok++;
    else failed++;
    o.log(pass ? `ok   ${name}` : `FAIL ${name}: ${detail instanceof Error ? `${detail.name} ${detail.message}` : JSON.stringify(detail)}`);
  };
  const auth = { kind: "password" as const, username: o.username, password: o.password };
  const base = { url: o.url, vae: o.vae, transport: o.transport, tenant: o.tenant };
  // a wrong password: a typed, non-retryable error
  const bad = await connect({ ...base, auth: { ...auth, password: "wrong" } }).then(
    (c) => (c.close(), undefined),
    (e: unknown) => e,
  );
  check(
    "a wrong password is a typed IAM error, not retryable",
    bad instanceof UbosError && bad.area === "IAM" && !bad.retryable && (bad instanceof IamDenied || bad.code.startsWith("IAM.")),
    bad,
  );
  let c: Client;
  try {
    c = await connect({ ...base, auth });
  } catch (e) {
    check("connect", false, e);
    return { ok, failed };
  }
  check("connected, protocol 1.0 negotiated", c.version === "1.0", c.session);
  const n0 = o.seed;
  const changes = Array.from({ length: 25 }, (_, i) => ({
    op: "create",
    class: "Customer",
    object_id: oid(n0 + i),
    slug: `t${n0 + i}`,
    body: { name: `tour ${i}`, email: `c${i}@example.com`, credit_limit: new Money("1250.50", "USD") },
  }));
  const r = await c.commit(changes).catch((e: unknown) => e);
  check("25 objects committed", !(r instanceof Error), r);
  const f = await c
    .commit([{ op: "create", class: "Customer", object_id: oid(n0 + 99), slug: "float", body: { name: "x", credit_limit: { amount: 12.5, currency: "USD" } } }])
    .catch((e: unknown) => e);
  check("a float refused at the API boundary (LogicFloatToDecimal)", f instanceof LogicFloatToDecimal, f);
  // idempotency: the same key returns the first result
  const key = uuid7();
  const v0 = (await c.get(`ubos://${o.vae}/Customer/t${n0}`)).version;
  const ch = [{ op: "update", object_id: oid(n0), base: v0, set: { name: "tour 0 renamed" } }];
  const opts = { idempotencyKey: key, trace: new TraceContext() };
  const r1 = await c.commit(ch, opts).catch((e: unknown) => e);
  const r2 = await c.commit(ch, opts).catch((e: unknown) => e);
  check(
    "the same idempotency key returns the first result",
    !(r1 instanceof Error) && !(r2 instanceof Error) && !!(r1 as { processId: string }).processId &&
      (r1 as { processId: string }).processId === (r2 as { processId: string }).processId,
    [r1, r2],
  );
  // reads with selectors and evidence
  const item = await c.get<{ credit_limit: { amount: string } }>(`ubos://${o.vae}/Customer/t${n0}`, { branch: "main" }).catch((e: unknown) => e);
  check(
    "a read with its version and evidence",
    item instanceof Item && item.version.startsWith("sha256:") && Object.keys(item.evidence).length > 0 && item.body.credit_limit.amount === "1250.50",
    item instanceof Item ? item.raw : item,
  );
  const missing = await c.get(`ubos://${o.vae}/Customer/no-such-${n0}`).catch((e: unknown) => e);
  check("a missing object is a typed error, not retried", missing instanceof UbosError && missing.area !== undefined && !missing.retryable, missing);
  // paging through snapshot cursors
  const items: Item[] = [];
  let qerr: unknown;
  try {
    for await (const i of c.query('from Customer where this.name != ""', { pageSize: 10 })) items.push(i);
  } catch (e) {
    qerr = e;
  }
  check("a query streamed in pages of 10", !qerr && items.length >= 25, qerr ?? items.length);
  const snaps = new Set(items.map((i) => JSON.stringify(i.evidence)));
  check("every page of the same snapshot", snaps.size === 1, [...snaps]);
  // a subscription
  const target = `ubos://${o.vae}/Customer/t${n0}`;
  const sub = await c.subscribe(target);
  const rename = async (name: string) => {
    const it = await c.get(target);
    return c.commit([{ op: "update", object_id: oid(n0), base: it.version, set: { name } }]);
  };
  const u1 = await rename("tour 0 again").catch((e: unknown) => e);
  check("an update with its base", !(u1 instanceof Error), u1);
  const e1 = await sub.next(10000);
  check(
    "the change emitted to the subscription",
    e1?.kind === "change" && Array.isArray(e1.body.changes) && e1.body.changes.length > 0,
    e1,
  );
  // files
  const data = new Uint8Array(2_000_000);
  for (let i = 0; i < data.length; i++) data[i] = (Math.imul(i, 2654435761) >>> 13) & 0xff;
  let seen = 0;
  const up = await c.files
    .upload(new Blob([data], { type: "application/octet-stream" }), {
      name: "scan.bin",
      chunkSize: 512 * 1024,
      onProgress: (n) => (seen = n),
    })
    .catch((e: unknown) => e);
  check("an upload in chunks, verified by SHA-256", !(up instanceof Error) && (up as { reference: { size: number } }).reference.size === data.length && seen === data.length, up);
  if (!(up instanceof Error)) {
    const slug = `doc${n0}`;
    const reference = (up as { reference: unknown }).reference;
    const d = await c
      .commit([{ op: "create", class: "Document", object_id: oid(n0 + 50), slug, body: { title: "tour", passport: reference } }])
      .catch((e: unknown) => e);
    check("the file referred to by a blob field", !(d instanceof Error), d);
    const doc = `ubos://${o.vae}/Document/${slug}`;
    const whole = await c.files.download(doc, "passport").catch((e: unknown) => e);
    check("the whole file downloaded and verified", whole instanceof Uint8Array && whole.length === data.length && whole.every((x, i) => x === data[i]), whole);
    const part = await c.files.download(doc, "passport", { range: [1000, 5000] }).catch((e: unknown) => e);
    check("a range downloaded", part instanceof Uint8Array && part.length === 4000 && part.every((x, i) => x === data[1000 + i]), part);
  }
  if (o.restart) {
    await o.restart();
    // the next request reconnects, signs in again and resubscribes
    const u2 = await rename("after restart").catch((e: unknown) => e);
    check("the client reconnected after the node restarted", !(u2 instanceof Error), u2);
    const e2 = await sub.next(15000);
    check("the subscription resumed: the change after the restart arrived", e2?.kind === "change", e2);
  }
  check("the subscription's cursor", typeof sub.cursor === "string", sub.cursor);
  const closed = await sub.close().catch((e: unknown) => e);
  check("unsubscribed", !(closed instanceof Error), closed);
  c.close();
  o.log(`\n${o.transport}: ${ok} ok, ${failed} failed`);
  return { ok, failed };
}
