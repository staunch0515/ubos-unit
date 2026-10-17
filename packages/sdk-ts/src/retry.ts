// The retry policy (DSN-SDK-003): only errors the registry classifies retryable (and
// transport failures) are retried, with exponential back-off and full jitter, within the
// caller's deadline.

import { type UbosError, TransportError } from "./error.js";
import { sleep } from "./util.js";

/** How a call is retried. */
export interface RetryPolicy {
  /** Attempts in all (1: no retry). Default 6. */
  maxAttempts: number;
  /** The first back-off in milliseconds. Default 100. */
  baseMs: number;
  /** The longest back-off in milliseconds. Default 5,000. */
  capMs: number;
  /** The deadline of a call when the caller gives none, in milliseconds. Default 30,000. */
  deadlineMs: number;
}

/** The default policy. */
export const DEFAULT_RETRY: Readonly<RetryPolicy> = { maxAttempts: 6, baseMs: 100, capMs: 5000, deadlineMs: 30000 };

/** No retries. */
export const NO_RETRY: Readonly<RetryPolicy> = { ...DEFAULT_RETRY, maxAttempts: 1 };

/** The back-off before attempt `n` (from 1): a uniform draw in `[0, min(cap, base · 2^n)]`. */
export function backoff(p: RetryPolicy, n: number): number {
  return Math.floor(Math.random() * (Math.min(p.capMs, p.baseMs * 2 ** Math.min(n, 20)) + 1));
}

/** Run `op` until it succeeds, fails with an error that is not retryable, runs out of
 * attempts or would pass `deadline` (epoch milliseconds). `headMoved` says whether
 * `TXN.HEAD_MOVED` is retryable for this operation (re-executable: no `base` given). */
export async function withRetry<T>(
  p: RetryPolicy,
  deadline: number,
  headMoved: boolean,
  op: (attempt: number) => Promise<T>,
): Promise<T> {
  for (let attempt = 1; ; attempt++) {
    try {
      return await op(attempt);
    } catch (err) {
      const e = err as UbosError;
      const retry = e.code === "TXN.HEAD_MOVED" ? headMoved : e.retryable === true;
      const wait = backoff(p, attempt);
      if (!retry || attempt >= p.maxAttempts || Date.now() + wait >= deadline) throw err;
      await sleep(wait);
    }
  }
}

/** An error for a call whose deadline passed. */
export function deadlinePassed(): TransportError {
  return new TransportError("the deadline passed");
}
