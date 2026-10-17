// Run the browser tour in headless Chromium against a node:
//
//   node examples/browser-check.mjs URL VAE USER PASSWORD (ws|http) [--restart]
//
// The page is served from its own origin, http://127.0.0.1:PAGE_PORT (default 18300), which
// the node must list in `server.cors_origins`. With --restart the check prints RESTART and
// waits for a line on standard input while the node restarts. Needs `npm run examples`
// first and Chromium (CHROMIUM, default the Playwright build in /opt/pw-browsers).

import { createServer } from "node:http";
import { readFile } from "node:fs/promises";
import { createInterface } from "node:readline";
import { extname, join, normalize } from "node:path";
import { chromium } from "playwright-core";

const [url, vae, user, password, transport] = process.argv.slice(2);
const restart = process.argv.includes("--restart");
const root = join(import.meta.dirname, "..");
const types = { ".html": "text/html", ".js": "text/javascript" };
const server = createServer(async (req, res) => {
  const path = normalize(join(root, new URL(req.url, "http://x").pathname));
  try {
    const body = await readFile(path);
    res.writeHead(200, { "content-type": types[extname(path)] ?? "application/octet-stream" });
    res.end(body);
  } catch {
    res.writeHead(404).end();
  }
});
const port = Number(process.env.PAGE_PORT ?? 18300);
await new Promise((r) => server.listen(port, "127.0.0.1", r));
const origin = `http://127.0.0.1:${port}`;
const lines = createInterface({ input: process.stdin })[Symbol.asyncIterator]();
const browser = await chromium.launch({
  executablePath: process.env.CHROMIUM ?? "/opt/pw-browsers/chromium-1194/chrome-linux/chrome",
});
const page = await browser.newPage();
let restarting;
page.on("console", (m) => {
  console.log(m.text());
  if (m.text() === "RESTART") restarting = lines.next().then(() => page.evaluate(() => window.restarted()));
});
const hash = new URLSearchParams({ url, vae, user, password, transport, seed: String(process.pid % 100000 + 1), restart: restart ? "1" : "0" });
await page.goto(`${origin}/examples/browser.html#${hash}`);
await page.waitForFunction(() => window.tourResult !== undefined, null, { timeout: 180000, polling: 200 });
await restarting;
const r = await page.evaluate(() => window.tourResult);
await browser.close();
server.close();
process.exit(r.failed === 0 ? 0 : 1);
