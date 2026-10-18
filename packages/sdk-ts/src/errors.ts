// Errors from the node's frames and problems, as instances of their code's class.

import { CLASSES } from "./codes.js";
import { UbosError } from "./error.js";

/** The error of a code with a message and details, as an instance of the code's class. */
export function errorOf(code: string, message: string, details?: unknown): UbosError {
  const C = CLASSES[code];
  return C ? new C(message, details) : new UbosError(message, details, code);
}

/** The error of an `error` frame body or an RFC 9457 problem (`code`, `message` or
 * `detail`, `details` or `explanation`). */
export function fromBody(b: unknown): UbosError {
  const o = (b ?? {}) as Record<string, unknown>;
  const code = typeof o.code === "string" ? o.code : "INT.PROTOCOL_ERROR";
  const message = typeof o.message === "string" ? o.message : typeof o.detail === "string" ? o.detail : "";
  const details = o.details ?? o.explanation ?? null;
  const e = errorOf(code, message, details);
  if (typeof o.process_id === "string") e.processId = o.process_id;
  return e;
}
