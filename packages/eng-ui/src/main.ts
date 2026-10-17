// The engineering UI of PH-1b (WSP prototype, DEC-078 part 17f): a minimal web shell used
// for acceptance — lists, details, history and forms generated from the model — that
// reaches the node through the TypeScript SDK only. The model comes from `forge types`
// (`generated/model.json`); without it the UI still browses any class by name.

import {
  type Auth,
  type Binding,
  type BlobRef,
  type Client,
  connect,
  Decimal,
  type Item,
  type MutateResult,
  type Page,
  type Selectors,
  type Subscription,
  UbosError,
  uuid7,
} from "@ubos/sdk";
import { field, h, mount } from "./dom.js";
import { type ClassDecl, type FieldDecl, type Model, parseModel, tree } from "./model.js";

type Json = Record<string, unknown>;

interface Settings {
  url: string;
  vae: string;
  tenant: string;
  auth: "dev" | "password" | "api_key" | "token";
  username: string;
  transport: Binding;
  branch: string;
}

const KEY = "ubos-eng-ui";
const app = document.getElementById("app") as HTMLElement;
let client: Client | undefined;
let model: Model | undefined;
let live: Subscription | undefined;
const view = { time: "", asof: "" };
const settings: Settings = {
  url: "http://127.0.0.1:7070",
  vae: "acme",
  tenant: "",
  auth: "dev",
  username: "admin",
  transport: "ws",
  branch: "main",
};

function load(): void {
  try {
    Object.assign(settings, JSON.parse(localStorage.getItem(`${KEY}.settings`) ?? "{}"));
    const m = localStorage.getItem(`${KEY}.model`);
    if (m) model = parseModel(m);
  } catch {
    // storage unavailable or stale: defaults
  }
}

function save(): void {
  try {
    localStorage.setItem(`${KEY}.settings`, JSON.stringify(settings));
    if (model) localStorage.setItem(`${KEY}.model`, JSON.stringify(model));
  } catch {
    // storage unavailable
  }
}

const sel = (extra: Selectors = {}): Selectors => ({
  branch: settings.branch || "main",
  ...(view.time ? { time: view.time } : {}),
  ...(view.asof ? { asof: view.asof } : {}),
  ...extra,
});
const classOf = (name: string): ClassDecl | undefined => model?.classes.find((c) => c.name === name);
const objectUri = (cls: string, slug: string) => `ubos://${settings.vae}/${cls}/${slug}`;
const go = (hash: string) => {
  location.hash = hash;
};

function errorBox(e: unknown): HTMLElement {
  if (e instanceof UbosError) {
    const d = e.details === null || e.details === undefined ? "" : `\n${JSON.stringify(e.details, null, 2)}`;
    return h("div", { class: "error", "data-code": e.code }, `${e.code}${e.status ? ` (${e.status})` : ""}: ${e.message}${d}`);
  }
  return h("div", { class: "error" }, String((e as Error)?.message ?? e));
}

/** A field value as text, or a download button for a file. */
function show(v: unknown, f?: FieldDecl, item?: Item): Node {
  if (v === null || v === undefined) return h("span", { class: "muted" }, "—");
  const t = f?.type;
  if (t === "money" && typeof v === "object") {
    const m = v as { amount: string; currency: string };
    return document.createTextNode(`${m.amount} ${m.currency}`);
  }
  if (t === "blob" && typeof v === "object") {
    const b = v as BlobRef;
    return h(
      "span",
      null,
      `${b.name ?? b.content_id} (${b.size} bytes) `,
      item && f
        ? h("button", { onclick: () => download(item, f.name, b) }, "Download")
        : null,
    );
  }
  if (typeof v === "object") return h("span", { class: "mono" }, JSON.stringify(v));
  return document.createTextNode(String(v));
}

async function download(item: Item, fieldName: string, b: BlobRef): Promise<void> {
  if (!client) return;
  // the URI names the branch the object was read on (STD-ADDR-001 selectors)
  const uri = `${objectUri(item.class, item.slug)}?branch=${settings.branch || "main"}`;
  const bytes = await client.files.download(uri, fieldName);
  const a = h("a", { href: URL.createObjectURL(new Blob([bytes as Uint8Array<ArrayBuffer>], { type: b.media_type ?? "application/octet-stream" })), download: b.name ?? b.content_id });
  a.click();
}

// ---- connection ----

function connectView(): HTMLElement {
  const status = h("div");
  const form = h(
    "form",
    {
      class: "card form",
      onsubmit: async (ev: Event) => {
        ev.preventDefault();
        const v = (n: string) => field(form, n)?.value ?? "";
        Object.assign(settings, {
          url: v("url"),
          vae: v("vae"),
          tenant: v("tenant"),
          auth: v("auth"),
          username: v("username"),
          transport: v("transport"),
          branch: v("branch") || "main",
        });
        const secret = v("secret");
        const auth: Auth =
          settings.auth === "password"
            ? { kind: "password", username: settings.username, password: secret }
            : settings.auth === "api_key"
              ? { kind: "api_key", key: secret }
              : settings.auth === "token"
                ? { kind: "token", token: secret }
                : { kind: "dev" };
        mount(status, h("div", { class: "muted" }, "connecting…"));
        try {
          client?.close();
          client = await connect({ url: settings.url, vae: settings.vae, auth, transport: settings.transport, tenant: settings.tenant || undefined });
          save();
          go("#/");
          render();
        } catch (e) {
          mount(status, errorBox(e));
        }
      },
    },
    h("label", null, "Node URL"), h("input", { name: "url", value: settings.url, size: 40 }),
    h("label", null, "VAE"), h("input", { name: "vae", value: settings.vae }),
    h("label", null, "Tenant (optional)"), h("input", { name: "tenant", value: settings.tenant }),
    h("label", null, "Binding"),
    h("select", { name: "transport" }, ["ws", "http"].map((t) => h("option", { value: t, selected: settings.transport === t }, t))),
    h("label", null, "Credentials"),
    h("select", { name: "auth" }, [["dev", "dev preset (none)"], ["password", "password"], ["api_key", "API key"], ["token", "bearer token"]].map(([k, l]) => h("option", { value: k, selected: settings.auth === k }, l))),
    h("label", null, "User name"), h("input", { name: "username", value: settings.username }),
    h("label", null, "Password, key or token"), h("input", { name: "secret", type: "password", autocomplete: "off" }),
    h("label", null, "Branch"), h("input", { name: "branch", value: settings.branch }),
    h("span"), h("button", { class: "primary", type: "submit" }, "Connect"),
  );
  return h("section", null, h("h1", null, "Connect to a node"), form, status, modelCard());
}

function modelCard(): HTMLElement {
  const status = h("div", { class: "muted" }, model ? `model of ${model.project} loaded: ${model.classes.length} classes` : "no model loaded");
  const use = (text: string) => {
    try {
      model = parseModel(text);
      save();
      mount(status, `model of ${model.project} loaded: ${model.classes.length} classes`);
      render();
    } catch (e) {
      mount(status, errorBox(e));
    }
  };
  const url = h("input", { placeholder: "model.json URL", size: 40 });
  return h(
    "div",
    { class: "card" },
    h("b", null, "Model "),
    h("span", { class: "muted" }, "(generated/model.json of forge types or forge dev --types)"),
    h(
      "div",
      { class: "bar" },
      h("input", { type: "file", accept: ".json", "data-test": "model-file", onchange: async (ev: Event) => {
        const f = (ev.target as HTMLInputElement).files?.[0];
        if (f) use(await f.text());
      } }),
      url,
      h("button", { onclick: async () => use(await (await fetch(url.value)).text()) }, "Load"),
    ),
    status,
  );
}

// ---- shell ----

function header(): HTMLElement {
  const input = (name: "time" | "asof", label: string) =>
    h("label", null, label, h("input", { value: view[name], placeholder: name === "time" ? "2026-10-17T08:00:00.000000Z" : "2026-10-17", size: name === "time" ? 26 : 11, onchange: (ev: Event) => {
      view[name] = (ev.target as HTMLInputElement).value.trim();
      render();
    } }));
  return h(
    "header",
    null,
    h("b", null, "UBOS engineering UI"),
    h("span", { class: "muted" }, `${settings.vae} @ ${settings.url} (${settings.transport}, protocol ${client?.version ?? "—"})`),
    h("label", null, "branch", h("input", { value: settings.branch, size: 14, "data-test": "branch", onchange: (ev: Event) => {
      settings.branch = (ev.target as HTMLInputElement).value.trim() || "main";
      save();
      render();
    } })),
    input("time", "time"),
    input("asof", "asof"),
    h("button", { onclick: () => go("#/model") }, model ? `model: ${model.project}` : "load model"),
    h("button", { onclick: () => { client?.close(); client = undefined; go("#/"); render(); } }, "Disconnect"),
  );
}

function nav(active: string): HTMLElement {
  const open = h("input", { placeholder: "class or ubos:// URI", size: 22, onkeydown: (ev: KeyboardEvent) => {
    if (ev.key !== "Enter") return;
    const v = (ev.target as HTMLInputElement).value.trim();
    go(v.startsWith("ubos://") ? `#/object/${encodeURIComponent(v)}` : `#/class/${encodeURIComponent(v)}`);
  } });
  return h(
    "nav",
    null,
    open,
    h("h2", null, "Classes"),
    model
      ? tree(model).map(({ cls, depth }) =>
          h("a", { href: `#/class/${encodeURIComponent(cls.name)}`, class: active === cls.name ? "active" : undefined, style: `padding-left: ${6 + depth * 14}px` }, cls.label || cls.name),
        )
      : h("div", { class: "muted" }, "load a model to list its classes"),
  );
}

// ---- lists ----

async function listView(cls: string, params: URLSearchParams): Promise<HTMLElement> {
  const decl = classOf(cls);
  const where = params.get("where") ?? "";
  const only = params.get("only") === "1";
  const out = h("div");
  const query: Json = { from: cls, limit: 25, ...(where ? { where } : {}), ...(only ? { only: true } : {}) };
  const stack: Page[] = [];
  const draw = (p: Page) => {
    const fields = decl ? decl.fields.slice(0, 6).map((f) => f.name) : [...new Set(p.items.flatMap((i) => Object.keys((i.body ?? {}) as Json)))].slice(0, 6);
    mount(
      out,
      h(
        "table",
        { "data-test": "list" },
        h("tr", null, h("th", null, "slug"), h("th", null, "class"), fields.map((f) => h("th", null, f)), h("th", null, "version")),
        p.items.map((i) => {
          const body = (i.body ?? {}) as Json;
          const d = classOf(i.class) ?? decl;
          return h(
            "tr",
            { class: "row", onclick: () => go(`#/object/${encodeURIComponent(objectUri(i.class, i.slug))}`) },
            h("td", null, i.slug), h("td", null, i.class),
            fields.map((f) => h("td", null, show(body[f], d?.fields.find((x) => x.name === f)))),
            h("td", { class: "mono" }, i.version.slice(7, 19)),
          );
        }),
      ),
      h(
        "div",
        { class: "bar" },
        h("span", { class: "muted" }, `${p.items.length} items, snapshot ${JSON.stringify(p.evaluatedAt)}`),
        h("button", { disabled: stack.length === 0, onclick: () => { const prev = stack.pop(); if (prev) draw(prev); } }, "Previous"),
        h("button", { disabled: p.cursor === undefined, onclick: async () => {
          try {
            const n = await p.next();
            if (n) {
              stack.push(p);
              draw(n);
            }
          } catch (e) {
            out.append(errorBox(e));
          }
        } }, "Next"),
      ),
    );
  };
  try {
    if (!client) throw new Error("not connected");
    draw(await client.queryPage(query, sel(), 25));
  } catch (e) {
    mount(out, errorBox(e));
  }
  const whereInput = h("input", { value: where, size: 48, placeholder: 'where, e.g. this.name == "Acme"' });
  const onlyBox = h("input", { type: "checkbox", checked: only });
  return h(
    "section",
    null,
    h("h1", null, decl?.label ?? cls),
    h(
      "div",
      { class: "bar" },
      decl ? h("button", { class: "primary", "data-test": "new", onclick: () => go(`#/new/${encodeURIComponent(cls)}`) }, `New ${cls}`) : null,
      whereInput,
      h("label", null, onlyBox, " without subclasses"),
      h("button", { onclick: () => go(`#/class/${encodeURIComponent(cls)}?${new URLSearchParams({ where: whereInput.value, only: onlyBox.checked ? "1" : "0" })}`) }, "Filter"),
    ),
    out,
  );
}

// ---- details ----

async function detailView(uri: string, params: URLSearchParams): Promise<HTMLElement> {
  if (!client) throw new Error("not connected");
  const commit = params.get("commit") ?? undefined;
  const item = await client.get<Json>(uri, sel(commit ? { commit } : {}));
  const decl = classOf(item.class);
  const body = item.body ?? {};
  const names = [...(decl?.fields.map((f) => f.name) ?? []), ...Object.keys(body).filter((k) => !decl?.fields.some((f) => f.name === k))];
  const result = h("div");
  const fieldsTable = h(
    "table",
    { "data-test": "fields" },
    names.map((n) => {
      const f = decl?.fields.find((x) => x.name === n);
      return h("tr", null, h("th", null, f?.label ?? n, f?.declared_by && f.declared_by !== item.class ? h("span", { class: "muted" }, ` (${f.declared_by})`) : null), h("td", { "data-field": n }, show(body[n], f, item)));
    }),
  );
  const ports = decl?.ports ?? [];
  const portForms = ports.map((p) => {
    const form = h("form", { class: "form", onsubmit: async (ev: Event) => {
      ev.preventDefault();
      const args: Json = {};
      for (const prm of p.params) {
        const v = field(form, prm.name)?.value ?? "";
        if (v !== "") args[prm.name] = prm.type === "integer" ? Number.parseInt(v, 10) : v;
      }
      try {
        const r = await client?.invoke(uri, p.name, args, { branch: settings.branch });
        mount(result, h("div", { class: "ok", "data-test": "port-result" }, `${p.name}: ${r?.outcome} — result ${JSON.stringify(r?.result)}, commits ${r?.commits.length}`));
        await refresh();
      } catch (e) {
        mount(result, errorBox(e));
      }
    } }, p.params.map((prm) => [h("label", { class: prm.required ? "req" : undefined }, prm.label ?? prm.name), h("input", { name: prm.name })]), h("span"), h("button", { type: "submit", "data-test": `port-${p.name}` }, `${p.kind} ${p.name}`));
    return h("div", { class: "card" }, form);
  });
  const explainProp = h("input", { placeholder: "property", size: 16 });
  const sec = h(
    "section",
    null,
    h("h1", null, `${item.class} `, h("span", { class: "mono" }, item.slug)),
    h(
      "div",
      { class: "bar" },
      h("button", { class: "primary", "data-test": "edit", onclick: () => go(`#/edit/${encodeURIComponent(uri)}`) }, "Edit"),
      h("button", { "data-test": "history", onclick: () => go(`#/history/${encodeURIComponent(uri)}`) }, "History"),
      h("button", { onclick: () => go(`#/class/${encodeURIComponent(item.class)}`) }, `All ${item.class}`),
      commit ? h("span", { class: "muted" }, `at commit ${commit}`) : null,
    ),
    h("div", { class: "card", id: "object" }, fieldsTable),
    h(
      "div",
      { class: "card mono" },
      h("div", null, `object ${item.objectId}`),
      h("div", null, `version ${item.version}`),
      h("div", null, `evidence ${JSON.stringify(item.evidence)}`),
      h("div", null, `valid ${JSON.stringify(item.raw.valid ?? null)}`),
    ),
    result,
    ports.length ? h("h2", null, "Ports") : null,
    portForms,
    h("h2", null, "Explain"),
    h(
      "div",
      { class: "bar" },
      ports.map((p) => h("button", { onclick: () => explain({ target: uri, kind: "decision", port: p.name }) }, `decision of ${p.name}`)),
      explainProp,
      h("button", { onclick: () => explain({ target: uri, kind: "cascade", property: explainProp.value }) }, "cascade"),
    ),
  );
  async function explain(x: Json): Promise<void> {
    try {
      const r = await client?.explain(x, sel());
      mount(result, h("div", { class: "card" }, h("pre", { class: "mono" }, JSON.stringify(r, null, 2))));
    } catch (e) {
      mount(result, errorBox(e));
    }
  }
  async function refresh(): Promise<void> {
    if (!client || commit) return;
    try {
      const fresh = await client.get<Json>(uri, sel());
      for (const n of names) {
        const cell = fieldsTable.querySelector(`[data-field="${CSS.escape(n)}"]`);
        if (cell) mount(cell, show(fresh.body?.[n], decl?.fields.find((x) => x.name === n), fresh));
      }
      fieldsTable.classList.remove("flash");
      void fieldsTable.offsetWidth;
      fieldsTable.classList.add("flash");
    } catch {
      // the next update tries again
    }
  }
  // live: the object's changes refresh the fields (IF-SDK-005)
  if (!commit && !view.time) {
    try {
      live = await client.subscribe(uri, (u) => {
        if (u.kind === "change") void refresh();
      }, { branch: settings.branch });
    } catch {
      // read-only views work without it
    }
  }
  return sec;
}

// ---- history ----

async function historyView(uri: string): Promise<HTMLElement> {
  if (!client) throw new Error("not connected");
  const r = await client.history(uri, sel());
  const items = (r.items ?? []) as Json[];
  const cols = [...new Set(items.flatMap((i) => Object.keys(i)))].filter((k) => typeof items[0]?.[k] !== "object").slice(0, 7);
  return h(
    "section",
    null,
    h("h1", null, "History ", h("span", { class: "mono" }, uri)),
    h("div", { class: "bar" }, h("button", { onclick: () => go(`#/object/${encodeURIComponent(uri)}`) }, "Back to the object")),
    h(
      "table",
      { "data-test": "history-table" },
      h("tr", null, cols.map((c) => h("th", null, c))),
      items.map((i) => {
        const commit = (i.commit_id ?? i.commit) as string | undefined;
        return h("tr", { class: "row", onclick: () => commit && go(`#/object/${encodeURIComponent(uri)}?commit=${encodeURIComponent(commit)}`) }, cols.map((c) => h("td", { class: "mono" }, String(i[c] ?? ""))));
      }),
    ),
    h("p", { class: "muted" }, `${items.length} commits; a row shows the object at that commit`),
  );
}

// ---- forms ----

function input(f: FieldDecl, value: unknown): HTMLElement {
  const name = f.name;
  switch (f.type) {
    case "enum":
      return h("select", { name }, h("option", { value: "" }, "—"), (f.constraints?.allowed_values ?? []).map((v) => h("option", { value: v, selected: value === v }, v)));
    case "boolean":
      return h("select", { name }, h("option", { value: "" }, "—"), ["true", "false"].map((v) => h("option", { value: v, selected: String(value) === v }, v)));
    case "integer":
      return h("input", { name, type: "number", step: 1, value: value ?? "" });
    case "date":
      return h("input", { name, type: "date", value: value ?? "" });
    case "money": {
      const m = (value ?? {}) as { amount?: string; currency?: string };
      return h("span", null, h("input", { name: `${name}.amount`, value: m.amount ?? "", placeholder: "amount", size: 14 }), " ", h("input", { name: `${name}.currency`, value: m.currency ?? "", placeholder: "USD", size: 4 }));
    }
    case "blob":
      return h("span", null, value ? show(value, f) : null, h("input", { name, type: "file" }));
    case "list":
    case "map":
    case "json":
    case "period":
      return h("textarea", { name, placeholder: "JSON" }, value === undefined || value === null ? "" : JSON.stringify(value, null, 2));
    default:
      return h("input", { name, value: value ?? "", size: 40, maxlength: f.constraints?.max_length });
  }
}

/** The body the form describes: values parsed by type; empty optional fields left out. */
async function collect(form: HTMLElement, fields: FieldDecl[], before: Json): Promise<Json> {
  const body: Json = {};
  for (const f of fields) {
    let v: unknown;
    if (f.type === "money") {
      const amount = field(form, `${f.name}.amount`)?.value.trim() ?? "";
      const currency = field(form, `${f.name}.currency`)?.value.trim() ?? "";
      v = amount || currency ? { amount: new Decimal(amount).toString(), currency } : undefined;
    } else if (f.type === "blob") {
      const file = (field(form, f.name) as HTMLInputElement | null)?.files?.[0];
      v = file && client ? (await client.files.upload(file, { name: file.name, mediaType: file.type || undefined })).reference : undefined;
      if (v === undefined) continue;
    } else {
      const raw = field(form, f.name)?.value.trim() ?? "";
      if (raw === "") v = undefined;
      else if (f.type === "integer") v = Number.parseInt(raw, 10);
      else if (f.type === "decimal") v = new Decimal(raw).toString();
      else if (f.type === "boolean") v = raw === "true";
      else if (["list", "map", "json", "period"].includes(f.type)) v = JSON.parse(raw) as unknown;
      else v = raw;
    }
    if (v === undefined) {
      if (before[f.name] !== undefined && before[f.name] !== null) body[f.name] = null;
      continue;
    }
    if (JSON.stringify(v) !== JSON.stringify(before[f.name])) body[f.name] = v;
  }
  return body;
}

async function formView(cls: string, uri?: string): Promise<HTMLElement> {
  if (!client) throw new Error("not connected");
  const existing = uri ? await client.get<Json>(uri, sel()) : undefined;
  const decl = classOf(existing?.class ?? cls);
  if (!decl) throw new Error(`no model of ${cls}: load the project's model.json`);
  const before = existing?.body ?? {};
  const status = h("div");
  const form = h(
    "form",
    {
      class: "form",
      "data-test": "object-form",
      onsubmit: async (ev: Event) => {
        ev.preventDefault();
        const c = client;
        if (!c) return;
        try {
          const body = await collect(form, decl.fields, before);
          let r: MutateResult;
          let target: string;
          if (existing) {
            r = await c.commit([{ op: "update", object_id: existing.objectId, base: existing.version, body }], { branch: settings.branch });
            target = uri ?? "";
          } else {
            const slug = field(form, "__slug")?.value.trim() ?? "";
            if (!slug) throw new Error("a slug is required");
            r = await c.commit([{ op: "create", class: decl.name, object_id: uuid7(), slug, body }], { branch: settings.branch });
            target = objectUri(decl.name, slug);
          }
          mount(status, h("div", { class: "ok" }, `committed ${r.commitId ?? ""}`));
          go(`#/object/${encodeURIComponent(target)}`);
        } catch (e) {
          mount(status, errorBox(e));
        }
      },
    },
    existing ? null : [h("label", { class: "req" }, "slug"), h("input", { name: "__slug", size: 30 })],
    decl.fields.map((f) => [
      h("label", { class: f.required ? "req" : undefined, title: `${f.type}${f.declared_by !== decl.name ? `, from ${f.declared_by}` : ""}` }, f.label || f.name),
      input(f, before[f.name]),
    ]),
    h("span"),
    h("div", { class: "bar" }, h("button", { class: "primary", type: "submit", "data-test": "save" }, existing ? "Save" : "Create"), h("button", { type: "button", onclick: () => history.back() }, "Cancel")),
  );
  return h("section", null, h("h1", null, existing ? `Edit ${decl.name} ${existing.slug}` : `New ${decl.name}`), h("div", { class: "card" }, form), status);
}

// ---- routing ----

async function render(): Promise<void> {
  await live?.close().catch(() => undefined);
  live = undefined;
  const [path, query = ""] = location.hash.replace(/^#/, "").split("?");
  const params = new URLSearchParams(query);
  const parts = path.split("/").filter(Boolean).map(decodeURIComponent);
  if (!client) {
    mount(app, connectView());
    return;
  }
  const body = h("div", null, h("section", { class: "muted" }, "loading…"));
  mount(app, header(), h("main", null, nav(parts[0] === "class" ? parts[1] : ""), body));
  try {
    let content: HTMLElement;
    switch (parts[0]) {
      case "class":
        content = await listView(parts[1], params);
        break;
      case "object":
        content = await detailView(parts[1], params);
        break;
      case "history":
        content = await historyView(parts[1]);
        break;
      case "new":
        content = await formView(parts[1]);
        break;
      case "edit":
        content = await formView("", parts[1]);
        break;
      case "model":
        content = h("section", null, modelCard());
        break;
      default:
        content = h(
          "section",
          null,
          h("h1", null, "Connected"),
          h("p", null, `Session ${JSON.stringify(client.session.principal ?? null)}; branch ${settings.branch}.`),
          model ? h("p", null, `Choose a class on the left (model ${model.model_hash.slice(0, 19)}…).`) : modelCard(),
        );
    }
    mount(body, content);
  } catch (e) {
    mount(body, h("section", null, errorBox(e)));
  }
}

load();
window.addEventListener("hashchange", () => void render());
void render();
