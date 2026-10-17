// The operations layer (DSN-SDK-002, -003, -005, -006; IF-SDK-001…003, -005): reads and
// queries with temporal and branch selectors, snapshot-consistent paging, mutations with
// automatic idempotency keys and the retry policy, and subscriptions that resume.

import { rejectFloats } from "./decimal.js";
import { errorOf } from "./errors.js";
import { withRetry } from "./retry.js";
import { type ConnectOptions, Session, type Update } from "./session.js";
import { Files } from "./files.js";
import { type TraceContext, uuid7 } from "./util.js";
import { Branches, ChangeSets, Inspect } from "./versioning.js";

/** The coordinates of a read (DSN-SDK-006, STD-ADDR-002); at most one of `time` and
 * `commit`. */
export interface Selectors {
  /** Branch (default `main`). */
  branch?: string;
  /** Valid time: a date or a canonical instant (`2026-10-17T08:00:00.000000Z`). */
  asof?: string;
  /** Transaction time (the snapshot at that instant): a canonical instant. */
  time?: string;
  /** A commit. */
  commit?: string;
}

/** Options of a call (IF-SDK-003). */
export interface CallOptions {
  /** Branch. */
  branch?: string;
  /** The idempotency key; generated (UUIDv7) when absent and reused on retries (DSN-SDK-002). */
  idempotencyKey?: string;
  /** The deadline of the call including its retries, in milliseconds from now. */
  deadlineMs?: number;
  /** The head `cseq` the caller saw: a moved head is `TXN.HEAD_MOVED`, not retried. */
  base?: number;
  /** A message for the process. */
  message?: string;
  /** The reason (audit). */
  reason?: string;
  /** The caller's trace context; the SDK sends a child span of it (DSN-SDK-016). */
  trace?: TraceContext;
}

/** What a subscription follows (STD-PROTO-012): an object URI, the objects of a query, or
 * released events of some types. */
export type Target = string | { object: string } | { query: unknown } | { events: string[] };

type Json = Record<string, unknown>;

function selectors(body: Json, s: Selectors | undefined): Json {
  for (const k of ["branch", "asof", "time", "commit"] as const) if (s?.[k] !== undefined) body[k] = s[k];
  return body;
}

/** A read item: the object version with its evidence (DSN-SDK-006). */
export class Item<B = Json> {
  constructor(
    /** The item as the node sent it, with `evaluated_at`. */
    readonly raw: Json,
  ) {}
  get objectId(): string {
    return String(this.raw.object_id ?? "");
  }
  get class(): string {
    return String(this.raw.class ?? "");
  }
  get slug(): string {
    return String(this.raw.slug ?? "");
  }
  /** The version identifier (`version_hash`). */
  get version(): string {
    return String(this.raw.version_hash ?? "");
  }
  /** The commit the version was read at, when the node names it. */
  get commit(): string | undefined {
    return typeof this.raw.commit === "string" ? this.raw.commit : undefined;
  }
  get body(): B {
    return this.raw.body as B;
  }
  /** The evaluation evidence: the coordinates the read was evaluated at. */
  get evidence(): Json {
    return (this.raw.evaluated_at ?? {}) as Json;
  }
}

/** The outcome of a mutation (IF-SDK-003). */
export class MutateResult {
  constructor(
    /** `{process_id, commits, versions, warnings, result}` or a pending change. */
    readonly raw: Json,
  ) {}
  /** `committed`, or `pending` when a decision requires approval. */
  get outcome(): "committed" | "pending" {
    return this.raw.pending ? "pending" : "committed";
  }
  get processId(): string {
    return String(this.raw.process_id ?? "");
  }
  /** The commits made. */
  get commits(): string[] {
    return (this.raw.commits ?? []) as string[];
  }
  /** The last commit made. */
  get commitId(): string | undefined {
    return this.commits.at(-1);
  }
  /** The new version of every object written. */
  get versions(): Record<string, string> {
    return (this.raw.versions ?? {}) as Record<string, string>;
  }
  /** The `cseq` of a raw commit. */
  get cseq(): number | undefined {
    const r = this.raw.result as Json | null;
    return typeof r?.cseq === "number" ? r.cseq : undefined;
  }
  /** The pending change of a `require` decision. */
  get pendingChangeId(): string | undefined {
    const p = this.raw.pending as Json | undefined;
    return typeof p?.pending_change_id === "string" ? p.pending_change_id : undefined;
  }
  /** The port's result. */
  get result(): unknown {
    return this.raw.result;
  }
  get warnings(): unknown[] {
    return (this.raw.warnings ?? []) as unknown[];
  }
}

/** One page of a query result; the pages of a query share the snapshot of the first. */
export class Page<B = Json> {
  constructor(
    readonly client: Client,
    readonly request: Json,
    /** The items. */
    readonly items: Item<B>[],
    /** The cursor of the next page, if any. */
    readonly cursor: unknown,
    /** The coordinates the query was evaluated at (the snapshot). */
    readonly evaluatedAt: Json,
  ) {}

  /** The next page, or `undefined` after the last. */
  async next(): Promise<Page<B> | undefined> {
    if (this.cursor === undefined || this.cursor === null) return undefined;
    const b = { ...this.request, query: { ...(this.request.query as Json), cursor: this.cursor } };
    return this.client.page<B>(b);
  }
}

/** A subscription: its updates in order (IF-SDK-005), by handler or by iteration. */
export class Subscription implements AsyncIterable<Update> {
  #queue: Update[] = [];
  #waiters: ((u: Update | undefined) => void)[] = [];
  #closed = false;

  constructor(
    readonly client: Client,
    /** @internal */
    readonly local: number,
    readonly handler?: (u: Update) => void,
  ) {}

  /** @internal Deliver an update. */
  push(u: Update): void {
    if (this.#closed) return;
    if (this.handler) {
      this.handler(u);
      return;
    }
    const w = this.#waiters.shift();
    if (w) w(u);
    else this.#queue.push(u);
  }

  /** The next update, or `undefined` when closed or after `timeoutMs`. */
  next(timeoutMs?: number): Promise<Update | undefined> {
    const u = this.#queue.shift();
    if (u || this.#closed) return Promise.resolve(u);
    return new Promise((resolve) => {
      const w = (x: Update | undefined) => {
        clearTimeout(timer);
        resolve(x);
      };
      const timer =
        timeoutMs === undefined
          ? undefined
          : setTimeout(() => {
              this.#waiters = this.#waiters.filter((x) => x !== w);
              resolve(undefined);
            }, timeoutMs);
      this.#waiters.push(w);
    });
  }

  async *[Symbol.asyncIterator](): AsyncIterator<Update> {
    for (;;) {
      const u = await this.next();
      if (!u) return;
      yield u;
    }
  }

  /** The cursor of the last update received (the resume point). */
  get cursor(): string | undefined {
    return this.client.subscription(this.local)?.cursor;
  }

  /** End the subscription (`unsubscribe`). */
  async close(): Promise<void> {
    this.#closed = true;
    for (const w of this.#waiters.splice(0)) w(undefined);
    const id = this.client.endSubscription(this.local);
    if (id) await this.client.request("unsubscribe", { subscription_id: id });
  }
}

/** A connected client (IF-SDK-001…006). */
export class Client extends Session {
  /** Uploads and downloads (IF-SDK-006). */
  readonly files: Files = new Files(this);
  /** Branches (IF-SDK-004). */
  readonly branches: Branches = new Branches(this);
  /** Change sets (IF-SDK-004). */
  readonly changeSets: ChangeSets = new ChangeSets(this);
  /** Introspection queries (STD-PROTO-015). */
  readonly inspect: Inspect = new Inspect(this);
  #nextSub = 1;

  #deadline(ms?: number): number {
    return Date.now() + (ms ?? this.retry.deadlineMs);
  }

  #body(extra: Json): Json {
    return { vae: this.options.vae, ...extra };
  }

  /** A query request with retries of transport failures and retryable errors. */
  async #ask(body: Json): Promise<Json> {
    const deadline = this.#deadline();
    return withRetry(this.retry, deadline, true, () => this.requestUntil("query", body, undefined, deadline));
  }

  /** Read one object (`ubos://vae/Class/slug`) with its version and evidence. */
  async get<B = Json>(uri: string, sel?: Selectors): Promise<Item<B>> {
    const r = await this.#ask(selectors(this.#body({ read: uri }), sel));
    const item = (r.items as Json[] | undefined)?.[0];
    if (!item) throw errorOf("IAM.NOT_FOUND", "no such object", { uri });
    return new Item<B>({ ...item, evaluated_at: r.evaluated_at });
  }

  /** @internal One page of a query request. */
  async page<B>(b: Json): Promise<Page<B>> {
    const r = await this.#ask(b);
    const ev = (r.evaluated_at ?? {}) as Json;
    const items = ((r.items ?? []) as Json[]).map((i) => new Item<B>({ ...i, evaluated_at: ev }));
    return new Page<B>(this, b, items, r.cursor ?? undefined, ev);
  }

  /** The first page of a query (its text or a `DAT-Query` object) with up to `limit`
   * items; follow Page.next() for the rest (DSN-SDK-005). */
  queryPage<B = Json>(query: string | Json, sel?: Selectors, limit = 100): Promise<Page<B>> {
    const q = typeof query === "string" ? { text: query, limit } : { limit, ...query };
    return this.page<B>(selectors(this.#body({ query: q }), sel));
  }

  /** Every item of a query, following the cursors within one snapshot (DSN-SDK-005). */
  async *query<B = Json>(query: string | Json, sel?: Selectors & { pageSize?: number }): AsyncGenerator<Item<B>> {
    let p: Page<B> | undefined = await this.queryPage<B>(query, sel, sel?.pageSize ?? 100);
    while (p) {
      yield* p.items;
      p = await p.next();
    }
  }

  /** `history {target}`: the commits of an object. */
  history(target: string, sel?: Selectors): Promise<Json> {
    return this.#ask(selectors(this.#body({ history: { target } }), sel));
  }

  /** `timeline {target}`: the valid-time segments of an object. */
  timeline(target: string, sel?: Selectors): Promise<Json> {
    return this.#ask(selectors(this.#body({ timeline: { target } }), sel));
  }

  /** `diff {from, to}`: the differences between two coordinates, each a branch and a
   * commit sequence (`cseq`, as a mutation's result gives it). */
  diff(from: { branch?: string; cseq: number }, to: { branch?: string; cseq: number }): Promise<Json> {
    const c = (x: { branch?: string; cseq: number }) => ({ branch: x.branch ?? "main", time: x.cseq });
    return this.#ask(this.#body({ diff: { from: c(from), to: c(to) } }));
  }

  /** @internal An introspection query (STD-PROTO-015) at the coordinates. */
  inspectRequest(kind: string, request: Json, sel?: Selectors): Promise<Json> {
    return this.#ask(selectors(this.#body({ [kind]: request }), sel));
  }

  /** Merge branch `source` into the branch of the call options (`branch.merge`). */
  merge(source: string, o: CallOptions & { resolutions?: unknown } = {}): Promise<MutateResult> {
    const m: Json = { source };
    if (o.resolutions !== undefined) m.resolutions = o.resolutions;
    return this.mutate({ merge: m }, o);
  }

  /** The preview of merging `source` into the branch of the selectors. */
  mergePreview(source: string, sel?: Selectors): Promise<Json> {
    return this.inspect.mergePreview(source, sel);
  }

  /** Revert the commits of a process (`{process_id}`), a merged change set
   * (`{change_set}`) or one commit (`{time}` or `{commit}`) on the branch of the call
   * options, with `reversal_ids` and `dry_run` (`revert`). */
  revert(spec: Json, o?: CallOptions): Promise<MutateResult> {
    return this.mutate({ revert: spec }, o);
  }

  /** A bulk process (`{changes, max_changes?, mode?, staging?}` or `{resume}`) on the
   * branch of the call options (`commit.raw`). */
  bulk(spec: Json, o?: CallOptions): Promise<MutateResult> {
    return this.mutate({ bulk: spec }, o);
  }

  /** `explain {target, kind, property | port}`. */
  explain(explain: Json, sel?: Selectors): Promise<Json> {
    return this.#ask(selectors(this.#body({ explain }), sel));
  }

  /** A mutation (`port` or `commit` body) with an idempotency key reused on every retry;
   * `TXN.HEAD_MOVED` is retried only without a `base` (re-executable). */
  async mutate(payload: Json, o: CallOptions = {}): Promise<MutateResult> {
    rejectFloats(payload);
    const b = this.#body(JSON.parse(JSON.stringify(payload)) as Json);
    b.idempotency_key = o.idempotencyKey ?? uuid7();
    if (o.branch !== undefined) b.branch = o.branch;
    if (o.message !== undefined) b.message = o.message;
    if (o.reason !== undefined) b.reason = o.reason;
    if (o.base !== undefined) b.base = o.base;
    const deadline = this.#deadline(o.deadlineMs);
    const r = await withRetry(this.retry, deadline, o.base === undefined, () =>
      this.requestUntil("mutate", b, o.trace, deadline),
    );
    return new MutateResult(r);
  }

  /** Invoke a port of an object (IF-SDK-003). */
  invoke(target: string, port: string, params: Json = {}, o?: CallOptions): Promise<MutateResult> {
    return this.mutate({ port: { target, name: port, params } }, o);
  }

  /** Commit raw changes (principals with `commit.raw`: import, Forge). */
  commit(changes: Json[], o?: CallOptions): Promise<MutateResult> {
    return this.mutate({ commit: { changes } }, o);
  }

  /** Subscribe (IF-SDK-005): updates go to `handler`, or are iterated from the returned
   * subscription; it resumes after reconnections from its last cursor. */
  async subscribe(
    target: Target,
    handler?: (u: Update) => void,
    opts: { branch?: string; cursor?: string } = {},
  ): Promise<Subscription> {
    const b: Json =
      typeof target === "string"
        ? { target }
        : "object" in target
          ? { target: target.object }
          : "query" in target
            ? { query: target.query }
            : { events: target.events.length ? { types: target.events } : {} };
    b.vae = this.options.vae;
    if (opts.branch) b.branch = opts.branch;
    const r = await this.subscribeRaw(opts.cursor ? { ...b, from_cursor: opts.cursor } : b);
    const local = this.#nextSub++;
    const sub = new Subscription(this, local, handler);
    this.subs.set(local, {
      body: b,
      cursor: typeof r.cursor === "string" ? r.cursor : undefined,
      serverId: typeof r.subscription_id === "string" ? r.subscription_id : undefined,
      deliver: (u) => sub.push(u),
    });
    return sub;
  }

  /** @internal The state of a local subscription. */
  subscription(local: number): { cursor: string | undefined } | undefined {
    return this.subs.get(local);
  }

  /** @internal Forget a local subscription; its server identifier. */
  endSubscription(local: number): string | undefined {
    const s = this.subs.get(local);
    this.subs.delete(local);
    return s?.serverId;
  }
}

/** Connect: authenticate and hand-shake (IF-SDK-001; `IamDenied`, `IntVersionUnsupported`). */
export function connect(options: ConnectOptions): Promise<Client> {
  return new Client(options).open();
}
