// Typed errors (DSN-SDK-007): every error of the node is a UbosError (an instance of the
// class of its registered code, see codes.ts) with its area, details and the registry's
// retryable and catchable flags; failures of the transport are TransportError, retryable.

import { REGISTRY, type Area, type Code } from "./registry.js";

/** The code of a transport failure, not an error of the node. */
export const TRANSPORT = "SDK.TRANSPORT";

/** An error of the SDK or of the node. */
export class UbosError extends Error {
  /** The registered code of the class (the base class has none). */
  static readonly code: string = "";
  /** The code (`AREA.NAME`, or `SDK.TRANSPORT`). */
  readonly code: string;
  /** Code-specific details (the node's `details` or `explanation`). */
  readonly details: unknown;

  constructor(message: string, details?: unknown, code?: string) {
    const c = code ?? (new.target as typeof UbosError).code;
    super(message || c);
    this.code = c;
    this.details = details ?? null;
    this.name = className(c);
  }

  /** The area of the code. */
  get area(): Area | undefined {
    return this.code in REGISTRY ? (this.code.split(".")[0] as Area) : undefined;
  }

  /** Whether the registry classifies the code retryable (transport failures are). */
  get retryable(): boolean {
    return this.code === TRANSPORT || (REGISTRY[this.code as Code]?.[0] ?? false);
  }

  /** Whether logic may catch the code. */
  get catchable(): boolean {
    return REGISTRY[this.code as Code]?.[1] ?? false;
  }

  /** The HTTP status of the code. */
  get status(): number | undefined {
    return REGISTRY[this.code as Code]?.[2];
  }

  /** The `reason` member of the details. */
  get reason(): string | undefined {
    const d = this.details as { reason?: unknown } | null;
    return typeof d?.reason === "string" ? d.reason : undefined;
  }

  /** Whether the code is `code`. */
  is(code: Code): boolean {
    return this.code === code;
  }
}

/** A failure of the transport (connection refused, reset, timed out): retryable. */
export class TransportError extends UbosError {
  static override readonly code = TRANSPORT;
}

/** `VER.MERGE_CONFLICT` → `VerMergeConflict`. */
function className(code: string): string {
  return code
    .split(/[._]/)
    .map((p) => p.charAt(0) + p.slice(1).toLowerCase())
    .join("");
}
