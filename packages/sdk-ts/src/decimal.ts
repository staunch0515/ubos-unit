// Exact decimals and money (DSN-SDK-013, STD-FND-020, STD-FND-022): string-backed values
// in canonical form, never floating point; rejectFloats refuses a value that carries a
// non-integer number at the API boundary (LOGIC.FLOAT_TO_DECIMAL).

import { errorOf } from "./errors.js";

/** A decimal in the canonical form of STD-FND-022 (no exponent, no leading `+`, no
 * leading zeros, no negative zero, no trailing fraction zeros). */
export class Decimal {
  readonly #text: string;

  /** Parse a decimal string (or a safe integer) and put it in canonical form. */
  constructor(value: string | number | bigint) {
    if (typeof value === "number" && !Number.isSafeInteger(value)) {
      throw errorOf("LOGIC.FLOAT_TO_DECIMAL", "a floating-point number; give decimals as strings", { value });
    }
    const text = String(value);
    const m = /^(-?)(\d+)(?:\.(\d+))?$/.exec(text);
    if (!m) {
      throw errorOf("MODEL.INVALID_VALUE", "a decimal is [-]digits[.digits]", { reason: "decimal_syntax" });
    }
    const int = m[2].replace(/^0+/, "") || "0";
    const frac = (m[3] ?? "").replace(/0+$/, "");
    const zero = int === "0" && frac === "";
    this.#text = (m[1] && !zero ? "-" : "") + int + (frac ? `.${frac}` : "");
  }

  /** The canonical text. */
  toString(): string {
    return this.#text;
  }

  /** A decimal travels as its text. */
  toJSON(): string {
    return this.#text;
  }

  /** The text with exactly `scale` fraction digits; `undefined` when digits would be lost. */
  withScale(scale: number): string | undefined {
    const [int, frac = ""] = this.#text.split(".");
    if (frac.length > scale) return undefined;
    return scale === 0 ? int : `${int}.${frac.padEnd(scale, "0")}`;
  }

  /** Whether two decimals are equal. */
  equals(other: Decimal | string): boolean {
    return this.#text === new Decimal(String(other)).#text;
  }
}

/** An amount of money `{amount, currency}` (STD-FND-020). */
export class Money {
  readonly amount: Decimal;
  readonly currency: string;

  constructor(amount: Decimal | string | number | bigint, currency: string) {
    if (!/^[A-Z]{3}$/.test(currency)) {
      throw errorOf("MODEL.INVALID_VALUE", "a currency is an ISO 4217 code", { reason: "currency" });
    }
    this.amount = amount instanceof Decimal ? amount : new Decimal(amount);
    this.currency = currency;
  }

  toJSON(): { amount: string; currency: string } {
    return { amount: this.amount.toString(), currency: this.currency };
  }
}

/** Refuse a value carrying a non-integer number anywhere (`LOGIC.FLOAT_TO_DECIMAL` with
 * the path): decimals and money travel as strings (DSN-SDK-013). */
export function rejectFloats(v: unknown, path: string[] = []): void {
  if (typeof v === "number") {
    if (!Number.isInteger(v)) {
      throw errorOf("LOGIC.FLOAT_TO_DECIMAL", "a floating-point number; send decimals as strings", {
        path: path.join("."),
      });
    }
    return;
  }
  if (v === null || typeof v !== "object") return;
  const j = v as { toJSON?: () => unknown };
  if (typeof j.toJSON === "function") {
    rejectFloats(j.toJSON(), path);
    return;
  }
  if (Array.isArray(v)) {
    v.forEach((x, i) => rejectFloats(x, [...path, String(i)]));
    return;
  }
  if (ArrayBuffer.isView(v)) return;
  for (const [k, x] of Object.entries(v)) rejectFloats(x, [...path, k]);
}
