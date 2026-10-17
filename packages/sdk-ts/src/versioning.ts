// Versioning operations and introspection (IF-SDK-004; BPA 1.0-rc4, STD-PROTO-014, -015;
// PH-1 step 18c, DEC-079): branches, change sets, merges and their previews, reverts and
// bulk processes as mutations with idempotency keys and the retry policy; the effective
// class, port dispatch, commit records, ledger entries, process records and replays as
// queries at the coordinates of Selectors.

import type { CallOptions, Client, MutateResult, Selectors } from "./client.js";

type Json = Record<string, unknown>;

/** Branches (`client.branches`). */
export class Branches {
  constructor(private readonly client: Client) {}

  /** Create branch `name` from branch `from` (default `main`) at its head (`branch.create`). */
  create(name: string, from = "main", o?: CallOptions): Promise<MutateResult> {
    return this.client.mutate({ create_branch: { name, from } }, o);
  }
}

/** A change set action (STD-VER-020, -021). */
export type ChangeSetAction = "get" | "submit" | "approve" | "reject" | "merge" | "abandon";

/** Change sets (`client.changeSets`); approvals are made by the session's principal. */
export class ChangeSets {
  constructor(private readonly client: Client) {}

  /** Open change set `id` (a UUID the caller chooses) from `source` into the branch of the
   * call options (default `main`). */
  open(id: string, source: string, title: string, o: CallOptions & { description?: string } = {}): Promise<MutateResult> {
    const cs: Json = { action: "open", id, source, title };
    if (o.branch !== undefined) cs.target = o.branch;
    if (o.description !== undefined) cs.description = o.description;
    return this.client.mutate({ change_set: cs }, o);
  }

  /** An action on change set `id`; `merge` takes optional resolutions. */
  act(id: string, action: ChangeSetAction, o: CallOptions & { resolutions?: unknown } = {}): Promise<MutateResult> {
    const cs: Json = { action, id };
    if (o.resolutions !== undefined) cs.resolutions = o.resolutions;
    return this.client.mutate({ change_set: cs }, o);
  }

  get(id: string, o?: CallOptions): Promise<MutateResult> {
    return this.act(id, "get", o);
  }
  submit(id: string, o?: CallOptions): Promise<MutateResult> {
    return this.act(id, "submit", o);
  }
  approve(id: string, o?: CallOptions): Promise<MutateResult> {
    return this.act(id, "approve", o);
  }
  reject(id: string, o?: CallOptions): Promise<MutateResult> {
    return this.act(id, "reject", o);
  }
  merge(id: string, o?: CallOptions & { resolutions?: unknown }): Promise<MutateResult> {
    return this.act(id, "merge", o);
  }
  abandon(id: string, o?: CallOptions): Promise<MutateResult> {
    return this.act(id, "abandon", o);
  }
}

/** Introspection queries (`client.inspect`). */
export class Inspect {
  constructor(private readonly client: Client) {}

  async #one(kind: string, request: Json, sel?: Selectors): Promise<Json> {
    const r = await this.client.inspectRequest(kind, request, sel);
    return ((r.items as Json[] | undefined) ?? [])[0] ?? {};
  }

  /** The effective class `name` at the coordinates, with the extensions in force. */
  class(name: string, sel?: Selectors): Promise<Json> {
    return this.#one("class", { name }, sel);
  }

  /** The class whose implementation of `port` runs for an object of `cls` (`{class, impl}`). */
  dispatch(cls: string, port: string, sel?: Selectors): Promise<Json> {
    return this.#one("dispatch", { class: cls, port }, sel);
  }

  /** The commit the coordinates select, with its state root. */
  commit(sel?: Selectors): Promise<Json> {
    return this.#one("commit_info", {}, sel);
  }

  /** The preview of merging the head of `source` into the head of the coordinates' branch. */
  mergePreview(source: string, sel?: Selectors): Promise<Json> {
    return this.#one("merge_preview", { source }, sel);
  }

  /** The entries of a ledger key in sequence order, filtered by `asof`. */
  async ledger(cls: string, ledgerKey: Json, sel?: Selectors): Promise<Json[]> {
    const r = await this.client.inspectRequest("ledger", { class: cls, ledger_key: ledgerKey }, sel);
    return (r.items as Json[] | undefined) ?? [];
  }

  /** The record of a process, with its journal and metering record when asked. */
  process(processId: string, o: { journal?: boolean; metering?: boolean } = {}): Promise<Json> {
    return this.#one("process", { process_id: processId, journal: !!o.journal, metering: !!o.metering });
  }

  /** Re-execute a process from its journal: `{outcome: "identical"}` or `{outcome: "diverged", divergence}`. */
  replay(processId: string): Promise<Json> {
    return this.#one("replay", { process_id: processId });
  }
}
