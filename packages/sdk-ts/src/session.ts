// The session layer (DSN-SDK-001, -004, -010, -014, -016): the handshake with version
// negotiation, the credentials, request–reply over the binding, and reconnection with
// re-authentication that resumes every subscription from its last acknowledged cursor.

import { TransportError, UbosError } from "./error.js";
import { errorOf, fromBody } from "./errors.js";
import { backoff, DEFAULT_RETRY, type RetryPolicy } from "./retry.js";
import { baseUrl, type Binding, type Frame, Link } from "./transport.js";
import { sleep, type TraceContext } from "./util.js";

/** The protocol versions this SDK speaks, newest first (DSN-SDK-014). */
export const VERSIONS = ["1.0"] as const;

/** How the SDK authenticates (DSN-SDK-010; PH-1: DEC-078 item 3). */
export type Auth =
  /** A local account: signed in at connection and again when the session ends. */
  | { kind: "password"; username: string; password: string }
  /** An API key. */
  | { kind: "api_key"; key: string }
  /** A bearer token obtained elsewhere. */
  | { kind: "token"; token: string }
  /** No credentials (a node in the `dev` preset). */
  | { kind: "dev" };

/** The options of a client (IF-SDK-001). */
export interface ConnectOptions {
  /** The node URL (`http[s]://host[:port][/prefix]`). */
  url: string;
  /** The VAE authority. */
  vae: string;
  /** The credentials. */
  auth: Auth;
  /** The binding (default `ws`). */
  transport?: Binding;
  /** The tenant when the node serves several. */
  tenant?: string;
  /** The retry policy (defaults: DEFAULT_RETRY). */
  retry?: Partial<RetryPolicy>;
  /** The timeout of one request in milliseconds (default 30,000). */
  requestTimeoutMs?: number;
}

/** An update a subscription delivers (IF-SDK-005). */
export type Update =
  /** The changes or events of one commit with its cursor. */
  | { kind: "change"; body: Record<string, unknown> }
  /** The resume point expired (`INT.CURSOR_EXPIRED`): the application refetches and the
   * subscription continues from the head. */
  | { kind: "reset"; error: UbosError };

/** @internal The state of a subscription across reconnections. */
export interface SubState {
  body: Record<string, unknown>;
  cursor: string | undefined;
  serverId: string | undefined;
  deliver: (u: Update) => void;
}

/** A session with a node: the layer below the operations, usable on its own. */
export class Session {
  /** The options. */
  readonly options: Readonly<ConnectOptions>;
  /** The retry policy. */
  readonly retry: Readonly<RetryPolicy>;
  /** The connection. */
  readonly link: Link;
  /** @internal */
  protected readonly subs = new Map<number, SubState>();
  #next = 1;
  #connected = false;
  #generation = 0;
  #reconnecting: Promise<void> | undefined;
  #token: string | undefined;
  #handshake: Record<string, unknown> = {};
  #acks: Promise<unknown> = Promise.resolve();
  #closed = false;
  #watchdog: ReturnType<typeof setInterval> | undefined;

  /** A session that is not open yet (see open()). */
  constructor(options: ConnectOptions) {
    this.options = { ...options };
    this.retry = { ...DEFAULT_RETRY, ...options.retry };
    this.link = new Link(options.transport ?? "ws", baseUrl(options.url), options.tenant, (f) => this.#route(f));
  }

  /** Authenticate and hand-shake (`IAM.DENIED`, `INT.VERSION_UNSUPPORTED`). */
  async open(): Promise<this> {
    await this.#establish();
    // reopen the session when its stream ended by itself while subscriptions are open, so
    // that they resume from their cursors without waiting for a request (DSN-SDK-004)
    this.#watchdog ??= setInterval(() => {
      if (this.#closed || !this.link.lost || ![...this.subs.values()].some((x) => x.serverId)) return;
      const seen = this.#generation;
      this.#connected = false;
      this.reconnect(seen, Date.now() + this.retry.deadlineMs).catch(() => undefined);
    }, 500);
    (this.#watchdog as { unref?: () => void }).unref?.();
    return this;
  }

  /** The handshake result: `{version, abi, session_id, principal, limits, features}`. */
  get session(): Readonly<Record<string, unknown>> {
    return this.#handshake;
  }

  /** The protocol version negotiated. */
  get version(): string {
    return String(this.#handshake.version ?? "");
  }

  /** The timeout of one request. */
  get timeoutMs(): number {
    return this.options.requestTimeoutMs ?? 30000;
  }

  /** The bearer credential for the HTTP APIs (files): the session token or API key. */
  bearer(): string | undefined {
    const a = this.options.auth;
    return a.kind === "api_key" ? a.key : a.kind === "token" ? a.token : a.kind === "password" ? this.#token : undefined;
  }

  /** The headers of the node's HTTP APIs: the bearer and the tenant. */
  apiHeaders(extra: Record<string, string> = {}): Record<string, string> {
    const h: Record<string, string> = { ...extra };
    const t = this.bearer();
    if (t) h.authorization = `Bearer ${t}`;
    if (this.options.tenant) h["ubtp-tenant"] = this.options.tenant;
    return h;
  }

  /** Close the connection; subscriptions end. */
  close(): void {
    clearInterval(this.#watchdog);
    this.#closed = true;
    this.#connected = false;
    this.link.reset();
    this.subs.clear();
  }

  #id(): string {
    return `c${this.#next++}`;
  }

  async #signIn(username: string, password: string): Promise<string> {
    let resp: Response;
    let v: Record<string, unknown>;
    try {
      resp = await fetch(`${this.link.base}/api/v1/identity/sign-in`, {
        method: "POST",
        headers: this.options.tenant
          ? { "content-type": "application/json", "ubtp-tenant": this.options.tenant }
          : { "content-type": "application/json" },
        body: JSON.stringify({ username, password }),
        signal: AbortSignal.timeout(this.timeoutMs),
      });
      v = (await resp.json().catch(() => ({}))) as Record<string, unknown>;
    } catch (e) {
      throw new TransportError(String((e as Error).message ?? e));
    }
    if (!resp.ok) throw fromBody(v);
    if (typeof v.token !== "string") throw errorOf("IAM.DENIED", "no token", v);
    return v.token;
  }

  async #credentials(): Promise<Record<string, unknown>> {
    const a = this.options.auth;
    switch (a.kind) {
      case "password":
        // a reconnection reuses the session token while the node accepts it
        this.#token ??= await this.#signIn(a.username, a.password);
        return { kind: "bearer", token: this.#token };
      case "api_key":
        return { kind: "api_key", key: a.key };
      case "token":
        return { kind: "bearer", token: a.token };
      case "dev":
        return { kind: "dev" };
    }
  }

  /** Open the session: credentials, handshake, then resubscribe. A held session token
   * the node refuses (expired, revoked) is dropped and the client signs in again. */
  async #establish(): Promise<void> {
    const held = this.options.auth.kind === "password" && this.#token !== undefined;
    try {
      await this.#establishOnce();
    } catch (e) {
      if (!held || !(e instanceof UbosError) || e instanceof TransportError || !e.code.startsWith("IAM.")) throw e;
      this.#token = undefined;
      await this.#establishOnce();
    }
  }

  async #establishOnce(): Promise<void> {
    if (this.#closed) throw new TransportError("the client is closed");
    this.link.reset();
    const credentials = await this.#credentials();
    const hs: Frame = {
      v: VERSIONS[0],
      id: this.#id(),
      type: "handshake",
      body: {
        vae: this.options.vae,
        versions: VERSIONS,
        abi: { min: "1.0", max: "1.0" },
        encodings: ["json"],
        features: [],
        credentials,
      },
    };
    const [f] = await this.link.exchange([hs], this.timeoutMs);
    if (!f) throw new TransportError("no handshake reply");
    if (f.type !== "result") throw fromBody(f.body);
    const body = (f.body ?? {}) as Record<string, unknown>;
    if (!(VERSIONS as readonly unknown[]).includes(body.version)) {
      throw errorOf("INT.VERSION_UNSUPPORTED", `the node chose ${String(body.version)}`, { supported: VERSIONS });
    }
    this.#handshake = body;
    this.#connected = true;
    this.#generation++;
    await this.#resubscribe();
  }

  /** Reconnect with back-off before `deadline` (DSN-SDK-004); concurrent callers wait for
   * one reconnection. */
  protected reconnect(seen: number, deadline: number): Promise<void> {
    if (this.#generation !== seen && this.#connected) return Promise.resolve();
    this.#reconnecting ??= (async () => {
      this.#connected = false;
      try {
        for (let attempt = 1; ; attempt++) {
          try {
            await this.#establish();
            return;
          } catch (e) {
            const wait = backoff(this.retry, attempt);
            if (!(e instanceof TransportError) || this.#closed || Date.now() + wait >= deadline) throw e;
            await sleep(wait);
          }
        }
      } finally {
        this.#reconnecting = undefined;
      }
    })();
    return this.#reconnecting;
  }

  /** One request and its reply body (`result`), or its error. A lost session (the node
   * restarted, the session expired) reconnects once and repeats the request. */
  request(type: string, body: Record<string, unknown>, trace?: TraceContext): Promise<Record<string, unknown>> {
    return this.requestUntil(type, body, trace, Date.now() + this.retry.deadlineMs);
  }

  /** @internal request() with the deadline of the call. */
  protected async requestUntil(
    type: string,
    body: Record<string, unknown>,
    trace: TraceContext | undefined,
    deadline: number,
  ): Promise<Record<string, unknown>> {
    for (let round = 0; round < 2; round++) {
      if (this.link.lost) this.#connected = false;
      if (!this.#connected) await this.reconnect(this.#generation, deadline);
      const m: Frame = { v: VERSIONS[0], id: this.#id(), type, body };
      if (trace) m.trace = trace.child().traceparent;
      let frames: Frame[];
      try {
        frames = await this.link.exchange([m], this.timeoutMs);
      } catch (e) {
        this.#connected = false;
        throw e;
      }
      const f = frames[0] ?? {};
      if (f.type === "result") return (f.body ?? {}) as Record<string, unknown>;
      const e = fromBody(f.body);
      const says = (w: string) => e.message.includes(w) || (typeof e.details === "string" && e.details.includes(w));
      const lost =
        (e.code === "INT.PROTOCOL_ERROR" && says("handshake")) ||
        (e.code === "IAM.NOT_FOUND" && says("session")) ||
        (e.code === "IAM.DENIED" && e.reason === "session_expired");
      if (lost && round === 0) {
        this.#connected = false;
        continue;
      }
      throw e;
    }
    throw new TransportError("the session could not be restored");
  }

  /** Route an `emit` frame to its subscription and acknowledge it (STD-PROTO-013). */
  #route(f: Frame): void {
    const b = (f.body ?? {}) as Record<string, unknown>;
    const sid = b.subscription_id;
    const cursor = b.cursor;
    if (typeof sid !== "string" || typeof cursor !== "string") return;
    for (const s of this.subs.values()) {
      if (s.serverId !== sid) continue;
      s.cursor = cursor;
      s.deliver({ kind: "change", body: b });
      // acknowledgements go in order, one after the other
      this.#acks = this.#acks
        .then(() => this.request("ack", { subscription_id: sid, cursor }))
        .catch(() => undefined);
      return;
    }
  }

  /** Subscribe again after a reconnection, each from its last acknowledged cursor; an
   * expired cursor delivers a reset and subscribes from the head. */
  async #resubscribe(): Promise<void> {
    for (const s of this.subs.values()) {
      if (!s.serverId) continue;
      let r: Record<string, unknown> | undefined;
      try {
        r = await this.subscribeRaw(s.cursor ? { ...s.body, from_cursor: s.cursor } : s.body);
      } catch (e) {
        if (e instanceof UbosError && e.code === "INT.CURSOR_EXPIRED") {
          s.deliver({ kind: "reset", error: e });
          r = await this.subscribeRaw(s.body).catch(() => undefined);
        }
      }
      if (r) {
        if (typeof r.subscription_id === "string") s.serverId = r.subscription_id;
        if (typeof r.cursor === "string") s.cursor = r.cursor;
      }
    }
  }

  /** @internal A `subscribe` request on the current connection. */
  protected async subscribeRaw(body: Record<string, unknown>): Promise<Record<string, unknown>> {
    const m: Frame = { v: VERSIONS[0], id: this.#id(), type: "subscribe", body: { vae: this.options.vae, ...body } };
    const [f] = await this.link.exchange([m], this.timeoutMs);
    if (f?.type === "result") return (f.body ?? {}) as Record<string, unknown>;
    throw fromBody(f?.body);
  }
}
