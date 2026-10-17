// Identifiers and trace context: UUIDv7 idempotency keys (DSN-SDK-002) and W3C trace
// context (DSN-SDK-016).

import { errorOf } from "./errors.js";

function random(n: number): Uint8Array {
  return globalThis.crypto.getRandomValues(new Uint8Array(n));
}

/** Lower-case hexadecimal. */
export function hex(b: Uint8Array): string {
  let s = "";
  for (const x of b) s += x.toString(16).padStart(2, "0");
  return s;
}

/** A UUIDv7 (RFC 9562): 48 bits of Unix milliseconds, then random bits. */
export function uuid7(): string {
  const b = random(16);
  let ms = Date.now();
  for (let i = 5; i >= 0; i--) {
    b[i] = ms % 256;
    ms = Math.floor(ms / 256);
  }
  b[6] = (b[6] & 0x0f) | 0x70;
  b[8] = (b[8] & 0x3f) | 0x80;
  const h = hex(b);
  return `${h.slice(0, 8)}-${h.slice(8, 12)}-${h.slice(12, 16)}-${h.slice(16, 20)}-${h.slice(20)}`;
}

/** A W3C trace context `00-<trace>-<span>-<flags>`. */
export class TraceContext {
  constructor(
    /** 32 hexadecimal digits. */
    readonly traceId: string = hex(random(16)),
    /** 16 hexadecimal digits. */
    readonly spanId: string = hex(random(8)),
    /** Sampled. */
    readonly sampled = true,
  ) {}

  /** Parse a `traceparent` value. */
  static parse(tp: string): TraceContext {
    const m = /^00-([0-9a-f]{32})-([0-9a-f]{16})-([0-9a-f]{2})$/i.exec(tp);
    if (!m) throw errorOf("MODEL.INVALID_VALUE", "a traceparent is 00-<32 hex>-<16 hex>-<2 hex>");
    return new TraceContext(m[1].toLowerCase(), m[2].toLowerCase(), m[3] === "01");
  }

  /** A child span of the same trace (the SDK's request inside the caller's span). */
  child(): TraceContext {
    return new TraceContext(this.traceId, hex(random(8)), this.sampled);
  }

  /** The `traceparent` value. */
  get traceparent(): string {
    return `00-${this.traceId}-${this.spanId}-${this.sampled ? "01" : "00"}`;
  }
}

/** Wait `ms` milliseconds. */
export function sleep(ms: number): Promise<void> {
  return new Promise((r) => setTimeout(r, ms));
}

/** Standard base64 of UTF-8 text (tus metadata). */
export function base64(text: string): string {
  let bin = "";
  for (const b of new TextEncoder().encode(text)) bin += String.fromCharCode(b);
  return btoa(bin);
}
