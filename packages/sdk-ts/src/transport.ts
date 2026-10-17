// The transport layer (DSN-SDK-001, STD-PROTO-020): the UBTP bindings of the Server over
// the platform's fetch and WebSocket, the same in browsers and Node.js.
//
// * "http": `POST /ubtp/v1` carries a list of messages and returns their frames; the
//   `Ubtp-Session` header keeps the session; `emit` frames arrive on the server-sent
//   events of `GET /ubtp/v1/stream?session=`, read with fetch.
// * "ws": `/ubtp/v1` with the subprotocol `ubtp.v1`, one message per frame; replies are
//   matched by `reply_to`, `emit` frames go to the session. The tenant travels as the
//   `tenant` query parameter (browsers cannot set WebSocket headers).
//
// The layer knows envelopes only; a lost connection is a TransportError the session layer
// reconnects from.

import { TransportError } from "./error.js";

/** A UBTP binding of the Server. */
export type Binding = "ws" | "http";

/** A UBTP frame or message. */
export type Frame = Record<string, unknown> & { type?: string; id?: string; reply_to?: string; body?: unknown };

/** `http[s]://host[:port][/prefix]` without a trailing slash (`ws[s]` accepted). */
export function baseUrl(url: string): string {
  const u = url.replace(/^ws(s?):\/\//, "http$1://").replace(/\/+$/, "");
  if (!/^https?:\/\/[^/]+/.test(u)) throw new TransportError("a node URL is http[s]://host[:port][/prefix]");
  return u;
}

interface Waiter {
  resolve: (f: Frame) => void;
  reject: (e: Error) => void;
}

/** The connection of one session over one binding. */
export class Link {
  #session: string | undefined;
  #ws: WebSocket | undefined;
  #opening: Promise<WebSocket> | undefined;
  #pending = new Map<string, Waiter>();
  #sse: AbortController | undefined;

  constructor(
    readonly binding: Binding,
    /** The base URL (see baseUrl). */
    readonly base: string,
    readonly tenant: string | undefined,
    /** Where `emit` frames go. */
    readonly onEmit: (f: Frame) => void,
  ) {}

  /** Drop the connection and the session (the next exchange opens anew). */
  reset(): void {
    this.#session = undefined;
    this.#sse?.abort();
    this.#sse = undefined;
    const ws = this.#ws;
    this.#ws = undefined;
    this.#opening = undefined;
    ws?.close();
    this.#fail(new TransportError("the connection was reset"));
  }

  /** Send messages and return the frames that reply to them, in order. */
  async exchange(messages: Frame[], timeoutMs: number): Promise<Frame[]> {
    return this.binding === "http" ? this.#http(messages, timeoutMs) : this.#websocket(messages, timeoutMs);
  }

  #fail(e: Error): void {
    const waiters = [...this.#pending.values()];
    this.#pending.clear();
    for (const w of waiters) w.reject(e);
  }

  async #http(messages: Frame[], timeoutMs: number): Promise<Frame[]> {
    const session = this.#session;
    const headers: Record<string, string> = { "content-type": "application/json" };
    if (session) headers["ubtp-session"] = session;
    if (this.tenant) headers["ubtp-tenant"] = this.tenant;
    let resp: Response;
    let v: { responses?: Frame[] };
    try {
      resp = await fetch(`${this.base}/ubtp/v1`, {
        method: "POST",
        headers,
        body: JSON.stringify(messages),
        signal: AbortSignal.timeout(timeoutMs),
      });
      v = (await resp.json()) as { responses?: Frame[] };
    } catch (e) {
      throw new TransportError(timedOut(e) ? "the request timed out" : String((e as Error).message ?? e));
    }
    const next = resp.headers.get("ubtp-session") ?? undefined;
    if (next) {
      this.#session = next;
      if (next !== session) this.#stream(next);
    }
    const out: Frame[] = [];
    for (const f of v.responses ?? []) {
      if (f.type === "emit") this.onEmit(f);
      else out.push(f);
    }
    return out;
  }

  /** Read the session's server-sent events into onEmit until reset. */
  #stream(session: string): void {
    this.#sse?.abort();
    const ac = new AbortController();
    this.#sse = ac;
    void (async () => {
      try {
        const resp = await fetch(`${this.base}/ubtp/v1/stream?session=${encodeURIComponent(session)}`, {
          headers: { accept: "text/event-stream" },
          signal: ac.signal,
        });
        if (!resp.ok || !resp.body) return;
        const reader = resp.body.pipeThrough(new TextDecoderStream()).getReader();
        let buf = "";
        for (;;) {
          const { value, done } = await reader.read();
          if (done) return;
          buf += value.replace(/\r\n/g, "\n");
          let i: number;
          while ((i = buf.indexOf("\n\n")) >= 0) {
            const event = buf.slice(0, i);
            buf = buf.slice(i + 2);
            const data = event
              .split("\n")
              .filter((l) => l.startsWith("data:"))
              .map((l) => l.slice(5).replace(/^ /, ""))
              .join("\n");
            if (!data) continue;
            try {
              this.onEmit(JSON.parse(data) as Frame);
            } catch {
              // not a frame
            }
          }
        }
      } catch {
        // aborted or lost: the next exchange restores the session
      }
    })();
  }

  #connect(timeoutMs: number): Promise<WebSocket> {
    if (this.#ws?.readyState === WebSocket.OPEN) return Promise.resolve(this.#ws);
    if (this.#opening) return this.#opening;
    const q = this.tenant ? `?tenant=${encodeURIComponent(this.tenant)}` : "";
    const opening = new Promise<WebSocket>((resolve, reject) => {
      let ws: WebSocket;
      try {
        ws = new WebSocket(`${this.base.replace(/^http/, "ws")}/ubtp/v1${q}`, "ubtp.v1");
      } catch (e) {
        reject(new TransportError(String((e as Error).message ?? e)));
        return;
      }
      const timer = setTimeout(() => {
        ws.close();
        reject(new TransportError("the connection timed out"));
      }, timeoutMs);
      ws.onopen = () => {
        clearTimeout(timer);
        resolve(ws);
      };
      ws.onerror = () => {
        clearTimeout(timer);
        reject(new TransportError("the WebSocket connection failed"));
      };
      ws.onclose = () => {
        clearTimeout(timer);
        if (this.#ws === ws) this.#ws = undefined;
        // every waiting request fails with a transport error
        this.#fail(new TransportError("the connection closed"));
      };
      ws.onmessage = (m) => {
        if (typeof m.data !== "string") return;
        let f: Frame;
        try {
          f = JSON.parse(m.data) as Frame;
        } catch {
          return;
        }
        const w = f.reply_to ? this.#pending.get(f.reply_to) : undefined;
        if (w && f.reply_to) {
          this.#pending.delete(f.reply_to);
          w.resolve(f);
        } else if (f.type === "emit") {
          this.onEmit(f);
        }
      };
    });
    this.#opening = opening;
    return opening.then(
      (ws) => {
        if (this.#opening === opening) {
          this.#opening = undefined;
          this.#ws = ws;
        }
        return ws;
      },
      (e: unknown) => {
        if (this.#opening === opening) this.#opening = undefined;
        throw e;
      },
    );
  }

  async #websocket(messages: Frame[], timeoutMs: number): Promise<Frame[]> {
    const ws = await this.#connect(timeoutMs);
    const waits = messages.map(
      (m) =>
        new Promise<Frame>((resolve, reject) => {
          const id = String(m.id);
          const timer = setTimeout(() => {
            this.#pending.delete(id);
            reject(new TransportError("the request timed out"));
          }, timeoutMs);
          this.#pending.set(id, {
            resolve: (f) => {
              clearTimeout(timer);
              resolve(f);
            },
            reject: (e) => {
              clearTimeout(timer);
              reject(e);
            },
          });
        }),
    );
    try {
      for (const m of messages) ws.send(JSON.stringify(m));
    } catch (e) {
      this.#ws = undefined;
      ws.close();
      throw new TransportError(String((e as Error).message ?? e));
    }
    return Promise.all(waits);
  }
}

function timedOut(e: unknown): boolean {
  return (e as Error)?.name === "TimeoutError" || (e as Error)?.name === "AbortError";
}
