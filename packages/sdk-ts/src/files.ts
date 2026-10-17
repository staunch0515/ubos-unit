// Files (DSN-SDK-011, IF-SDK-006): resumable tus uploads with progress and hash
// verification, and ranged downloads through the node's signed links (DSN-NOD-320, -327).

import { TransportError } from "./error.js";
import { errorOf, fromBody } from "./errors.js";
import { backoff } from "./retry.js";
import type { Session } from "./session.js";
import { Sha256 } from "./sha256.js";
import { base64, sleep } from "./util.js";

/** The value of a `blob` field (STD-FND-020). */
export interface BlobRef {
  /** The file identifier. */
  content_id: string;
  /** Size in bytes. */
  size: number;
  media_type?: string | null;
  name?: string | null;
}

/** Bytes to upload: a Blob or File (read chunk by chunk) or a buffer. */
export type UploadData = Blob | ArrayBuffer | Uint8Array;

/** Options of an upload. */
export interface UploadOptions {
  /** File name (a File's name by default). */
  name?: string;
  /** Media type (a Blob's type by default). */
  mediaType?: string;
  /** Bytes per request (default 4 MiB). */
  chunkSize?: number;
  /** Called with `(bytes sent, total)`. */
  onProgress?: (sent: number, total: number) => void;
  /** Resume this upload (its location) instead of creating one. */
  resume?: string;
  /** Called with the location once the upload exists (keep it to resume later). */
  onLocation?: (location: string) => void;
}

/** A download link: `{url, expires_in, file, size, sha256, classification}`. */
export interface FileLink {
  url: string;
  expires_in: number;
  file: string;
  size: number;
  sha256: string;
  classification: string;
}

async function slice(d: UploadData, start: number, end: number): Promise<Uint8Array> {
  if (d instanceof Blob) return new Uint8Array(await d.slice(start, end).arrayBuffer());
  const u = d instanceof Uint8Array ? d : new Uint8Array(d);
  return u.subarray(start, end);
}

/** The files API of a client. */
export class Files {
  constructor(readonly session: Session) {}

  async #http(method: string, path: string, headers: Record<string, string>, body?: Uint8Array | string) {
    try {
      return await fetch(path.startsWith("http") ? path : `${this.session.link.base}${path}`, {
        method,
        headers: this.session.apiHeaders({ "tus-resumable": "1.0.0", ...headers }),
        body: body as BodyInit | undefined,
        signal: AbortSignal.timeout(this.session.timeoutMs),
      });
    } catch (e) {
      throw new TransportError(String((e as Error).message ?? e));
    }
  }

  static async #problem(r: Response) {
    const v = await r.json().catch(() => ({ code: "INT.PROTOCOL_ERROR", detail: `HTTP ${r.status}` }));
    return fromBody(v);
  }

  /** The offset an upload resumes from (`HEAD`). */
  async offset(location: string): Promise<number> {
    const r = await this.#http("HEAD", location, {});
    if (r.status !== 200) throw await Files.#problem(r);
    const o = r.headers.get("upload-offset");
    if (o === null) throw new TransportError("no Upload-Offset");
    return Number(o);
  }

  /** Upload bytes as a file: create the upload (or resume `resume`), send the bytes in
   * chunks, resume from the node's offset after a transport failure, and check the node's
   * SHA-256 of the result against the bytes sent. */
  async upload(data: UploadData, o: UploadOptions = {}): Promise<{ reference: BlobRef; location: string }> {
    const total = data instanceof Blob ? data.size : data.byteLength;
    let location = o.resume;
    if (!location) {
      const meta: string[] = [];
      const name = o.name ?? (data instanceof Blob && "name" in data ? String(data.name) : undefined);
      const type = o.mediaType ?? (data instanceof Blob && data.type ? data.type : undefined);
      if (name) meta.push(`filename ${base64(name)}`);
      if (type) meta.push(`filetype ${base64(type)}`);
      const r = await this.#http("POST", `/files/v1/${this.session.options.vae}`, {
        "upload-length": String(total),
        "upload-metadata": meta.join(","),
      });
      if (r.status !== 201) throw await Files.#problem(r);
      location = r.headers.get("location") ?? undefined;
      if (!location) throw new TransportError("no Location");
    }
    o.onLocation?.(location);
    const chunk = Math.max(1, o.chunkSize ?? 4 << 20);
    const policy = this.session.retry;
    const deadline = Date.now() + Math.max(policy.deadlineMs, 300000);
    // the hash follows the bytes in order; a resumed upload hashes its prefix first
    const hash = new Sha256();
    let hashed = 0;
    const hashTo = async (end: number) => {
      for (; hashed < end; ) {
        const next = Math.min(end, hashed + chunk);
        hash.update(await slice(data, hashed, next));
        hashed = next;
      }
    };
    let offset = total === 0 ? 0 : await this.offset(location);
    for (let attempt = 0; offset < total; ) {
      const end = Math.min(offset + chunk, total);
      const bytes = await slice(data, offset, end);
      let r: Response;
      try {
        r = await this.#http(
          "PATCH",
          location,
          { "upload-offset": String(offset), "content-type": "application/offset+octet-stream" },
          bytes,
        );
      } catch (e) {
        attempt++;
        const wait = backoff(policy, attempt);
        if (Date.now() + wait >= deadline || attempt >= policy.maxAttempts) throw e;
        await sleep(wait);
        offset = await this.offset(location);
        continue;
      }
      if (r.status === 204) {
        offset = Number(r.headers.get("upload-offset") ?? end);
        attempt = 0;
        o.onProgress?.(offset, total);
      } else if (r.status === 409) {
        offset = await this.offset(location);
      } else {
        throw await Files.#problem(r);
      }
    }
    const r = await this.#http("GET", location, {});
    if (r.status !== 200) throw await Files.#problem(r);
    const v = (await r.json()) as { reference?: BlobRef; sha256?: string };
    if (!v.reference) throw new TransportError("the upload did not complete");
    await hashTo(total);
    if (v.sha256 && v.sha256 !== hash.hex()) {
      throw errorOf("NOD.BLOB_UNAVAILABLE", "the node's SHA-256 of the file differs from the bytes sent", {
        integrity: true,
      });
    }
    return { reference: v.reference, location };
  }

  /** A signed download link for the file of an object's `blob` field. */
  async link(object: string, field: string): Promise<FileLink> {
    const r = await this.#http(
      "POST",
      `/files/v1/${this.session.options.vae}/links`,
      { "content-type": "application/json" },
      JSON.stringify({ object, field }),
    );
    if (r.status !== 200) throw await Files.#problem(r);
    return (await r.json()) as FileLink;
  }

  /** Download the file of an object's `blob` field, whole (verified against its SHA-256)
   * or a byte range `[start, end)`. */
  async download(
    object: string,
    field: string,
    o: { range?: [number, number]; onProgress?: (received: number, total: number) => void } = {},
  ): Promise<Uint8Array> {
    const l = await this.link(object, field);
    const headers: Record<string, string> = {};
    if (o.range) headers.range = `bytes=${o.range[0]}-${Math.max(0, o.range[1] - 1)}`;
    let r: Response;
    try {
      r = await fetch(`${this.session.link.base}${l.url}`, { headers });
    } catch (e) {
      throw new TransportError(String((e as Error).message ?? e));
    }
    if (r.status !== 200 && r.status !== 206) throw await Files.#problem(r);
    const total = Number(r.headers.get("content-length") ?? 0);
    const parts: Uint8Array[] = [];
    const hash = new Sha256();
    let received = 0;
    if (r.body) {
      const reader = r.body.getReader();
      for (;;) {
        const { value, done } = await reader.read();
        if (done) break;
        parts.push(value);
        hash.update(value);
        received += value.length;
        o.onProgress?.(received, total);
      }
    }
    const sha = r.headers.get("ubos-content-sha256");
    if (!o.range && sha && hash.hex() !== sha) {
      throw errorOf("NOD.BLOB_UNAVAILABLE", "the downloaded bytes do not match the file's SHA-256", {
        integrity: true,
      });
    }
    const out = new Uint8Array(received);
    let at = 0;
    for (const p of parts) {
      out.set(p, at);
      at += p.length;
    }
    return out;
  }
}
