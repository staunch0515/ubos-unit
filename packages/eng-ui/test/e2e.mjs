// The engineering UI in headless Chromium against a `forge dev` node (PH-1 step 17f):
// connect, load the model, generated forms with a validation error and its correction, an
// edit, the history and the object at an earlier commit, a polymorphic list, a port call,
// a file upload, and a change by another client shown live.
//
//   node test/e2e.mjs --forge ../../target/debug/forge --server ../../target/debug/ubos-server
//        [--project ../../verify/sdk/fixture] [--binding ws|http] [--chromium PATH] [--screenshots DIR]
//
// Needs `npm run build` first. One line per check; exit status 1 when one fails.

import { spawn } from "node:child_process";
import { createServer } from "node:http";
import { readFile } from "node:fs/promises";
import { createInterface } from "node:readline";
import { extname, join, normalize, resolve } from "node:path";
import { chromium } from "playwright-core";

const args = Object.fromEntries(process.argv.slice(2).reduce((a, x, i, all) => (i % 2 ? a : [...a, [x.replace(/^--/, ""), all[i + 1]]]), []));
const here = import.meta.dirname;
const project = resolve(args.project ?? join(here, "../../../verify/sdk/fixture"));
const nodePort = 17300 + Math.floor(Math.random() * 500);
const nodeUrl = `http://127.0.0.1:${nodePort}`;
const binding = args.binding ?? "ws";
let ok = 0;
let failed = 0;
const check = (name, pass, detail) => {
  if (pass) ok++;
  else failed++;
  console.log(pass ? `ok   ${name}` : `FAIL ${name}: ${detail ?? ""}`);
};

// the node
const dev = spawn(args.forge, ["dev", project, "--json", "--listen", `127.0.0.1:${nodePort}`, "--server", args.server], { stdio: ["ignore", "pipe", "inherit"] });
const ready = await new Promise((res, rej) => {
  createInterface({ input: dev.stdout }).on("line", (l) => {
    try {
      const e = JSON.parse(l);
      if (e.event === "ready") res(e);
      if (e.event === "rejected") rej(new Error(l));
    } catch {
      // not an event
    }
  });
  dev.on("exit", (c) => rej(new Error(`forge dev exited with ${c}`)));
});
check("forge dev serves the project", ready.url === nodeUrl, JSON.stringify(ready));

// the page, from its own origin (allowed by forge dev)
const dist = join(here, "..", "dist");
const web = createServer(async (req, res) => {
  let p = normalize(join(dist, new URL(req.url, "http://x").pathname));
  if (p === dist || p.endsWith("/")) p = join(dist, "index.html");
  try {
    const body = await readFile(p);
    res.writeHead(200, { "content-type": { ".html": "text/html", ".js": "text/javascript" }[extname(p)] ?? "application/octet-stream" });
    res.end(body);
  } catch {
    res.writeHead(404).end();
  }
});
await new Promise((r) => web.listen(7080, "127.0.0.1", r));

const browser = await chromium.launch({ executablePath: args.chromium ?? process.env.CHROMIUM ?? "/opt/pw-browsers/chromium-1194/chrome-linux/chrome" });
const page = await browser.newPage();
page.on("pageerror", (e) => console.log("page error:", e.message));
const T = { timeout: 15000 };
await page.setViewportSize({ width: 1280, height: 760 });
const shot = async (name) => {
  if (args.screenshots) await page.screenshot({ path: join(args.screenshots, `${name}.png`) });
};

/** A UBTP request of another client (HTTP, dev credentials). */
async function ubtp(messages, session) {
  const r = await fetch(`${nodeUrl}/ubtp/v1`, { method: "POST", headers: { "content-type": "application/json", ...(session ? { "ubtp-session": session } : {}) }, body: JSON.stringify(messages) });
  return { session: r.headers.get("ubtp-session") ?? session, frames: (await r.json()).responses.filter((f) => f.type !== "emit") };
}

try {
  await page.goto("http://127.0.0.1:7080/");
  await page.fill('input[name="url"]', nodeUrl);
  await page.fill('input[name="vae"]', "acme");
  await page.selectOption('select[name="auth"]', "dev");
  await page.selectOption('select[name="transport"]', binding);
  await page.fill('input[name="branch"]', "feature/dev");
  await page.click('button[type="submit"]');
  await page.waitForSelector("text=UBOS engineering UI", T).catch(async (e) => { console.log("page:", (await page.innerHTML("#app")).slice(0, 2500)); throw e; });
  check(`connected over ${binding} with the SDK`, (await page.textContent("header")).includes(`(${binding}, protocol 1.0)`));

  await page.click("text=load model");
  await page.setInputFiles('[data-test="model-file"]', join(project, "generated", "model.json"));
  await page.waitForSelector("text=model of sdk-suite loaded", T);
  check("the model loaded: the class tree", await page.isVisible('nav >> text=Supplier'));

  // a generated form, a validation error, its correction
  await page.click("nav >> text=Customer");
  await page.click('[data-test="new"]', T);
  await page.fill('input[name="__slug"]', "cust-1");
  await page.fill('input[name="name"]', "Acme");
  await page.fill('input[name="rating"]', "9");
  await page.fill('input[name="credit_limit.amount"]', "100");
  await page.fill('input[name="credit_limit.currency"]', "USD");
  await page.click('[data-test="save"]');
  const err = await page.waitForSelector('.error[data-code="RULE.VALIDATION_FAILED"]', T).catch(() => null);
  check("a rule violation is shown with its code and details", err && (await err.textContent()).includes("rating_range"), err && (await err.textContent()));
  await shot("1-form-with-a-rule-violation");
  await page.fill('input[name="rating"]', "4");
  await page.click('[data-test="save"]');
  await page.waitForSelector('[data-test="fields"] >> text=Acme', T);
  const fields = await page.textContent('[data-test="fields"]');
  check("created through the generated form; money and inherited fields shown", fields.includes("USD") && fields.includes("(Party)"), fields);

  // an edit and the history
  await page.click('[data-test="edit"]');
  await page.fill('input[name="name"]', "Acme Inc.");
  await page.click('[data-test="save"]');
  await page.waitForSelector('[data-test="fields"] >> text=Acme Inc.', T);
  check("edited with its base version", true);
  const customerHash = await page.evaluate(() => location.hash);
  await page.click('[data-test="history"]');
  await page.waitForSelector('[data-test="history-table"] tr.row', T);
  const rows = await page.$$('[data-test="history-table"] tr.row');
  const names = new Set();
  for (let i = 0; i < rows.length; i++) {
    const list = await page.$$('[data-test="history-table"] tr.row');
    await list[i].click();
    await page.waitForSelector("text=at commit", T);
    names.add((await page.textContent('[data-field="name"]')).trim());
    await page.goBack();
    await page.waitForSelector('[data-test="history-table"] tr.row', T);
  }
  await shot("2-history");
  check("the history: two commits, each showing the object at that commit", rows.length === 2 && names.has("Acme") && names.has("Acme Inc."), [...names].join(" | "));

  // a polymorphic list
  await page.click("nav >> text=Party");
  await page.waitForSelector('[data-test="list"] >> text=cust-1', T);
  check("the Party list shows the Customer", (await page.textContent('[data-test="list"]')).includes("Customer"));
  await shot("3-polymorphic-list");

  // a port call
  await page.click("nav >> text=Account");
  await page.click('[data-test="new"]', T);
  await page.fill('input[name="__slug"]', "acct-1");
  await page.fill('input[name="name"]', "Ops");
  await page.click('[data-test="save"]');
  await page.waitForSelector('[data-test="port-rename"]', T);
  await page.fill('form:has([data-test="port-rename"]) input[name="name"]', "Operating");
  await page.click('[data-test="port-rename"]');
  const pr = await page.waitForSelector('[data-test="port-result"]', T).catch(() => null);
  await page.waitForSelector('[data-field="name"] >> text=Operating', T).catch(() => null);
  await shot("4-object-with-ports");
  check("a port invoked with its parameters; its effect shown", pr && (await page.textContent('[data-field="name"]')).includes("Operating"), pr && (await pr.textContent()));

  // a file
  await page.click("nav >> text=Document");
  await page.click('[data-test="new"]', T);
  await page.fill('input[name="__slug"]', "doc-1");
  await page.fill('input[name="title"]', "Passport");
  await page.setInputFiles('input[name="passport"]', { name: "scan.txt", mimeType: "text/plain", buffer: Buffer.from("a scanned passport\n".repeat(1000)) });
  await page.click('[data-test="save"]');
  await page.waitForSelector('[data-field="passport"] >> text=Download', T).catch(() => null);
  const pf = await page.textContent('[data-field="passport"]').catch(() => "");
  check("a file uploaded through the form and offered for download", pf.includes("scan.txt") && pf.includes("19000 bytes"), pf);
  const [dl] = await Promise.all([page.waitForEvent("download", T).catch(() => null), page.click('[data-field="passport"] >> text=Download')]);
  check("the file downloaded", dl && dl.suggestedFilename() === "scan.txt", dl && dl.suggestedFilename());

  // another client's change, shown live
  await page.goto(`http://127.0.0.1:7080/${customerHash}`);
  await page.waitForSelector('[data-field="name"] >> text=Acme Inc.', T);
  const hs = await ubtp([{ v: "1.0", id: "h", type: "handshake", body: { vae: "acme", versions: ["1.0"], credentials: { kind: "dev" } } }]);
  const read = await ubtp([{ v: "1.0", id: "r", type: "query", body: { vae: "acme", branch: "feature/dev", read: "ubos://acme/Customer/cust-1" } }], hs.session);
  const it = read.frames[0].body.items[0];
  await ubtp([{ v: "1.0", id: "m", type: "mutate", body: { vae: "acme", branch: "feature/dev", idempotency_key: String(Date.now()), commit: { changes: [{ op: "update", object_id: it.object_id, base: it.version_hash, body: { name: "Acme Live" } }] } } }], hs.session);
  const liveSeen = await page.waitForSelector('[data-field="name"] >> text=Acme Live', { timeout: 10000 }).then(() => true, () => false);
  check("another client's change appears without a reload (subscription)", liveSeen);
} catch (e) {
  check("the walk-through ran to its end", false, e.message);
} finally {
  await browser.close();
  web.close();
  dev.kill("SIGTERM");
}
console.log(`\n${ok} ok, ${failed} failed`);
process.exit(failed ? 1 : 0);
