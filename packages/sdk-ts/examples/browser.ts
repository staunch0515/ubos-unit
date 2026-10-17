// The tour in a browser page (examples/browser.html#url=…&vae=…&user=…&password=…&transport=ws).
// Every line goes to the console and the page; with `restart=1` the page logs RESTART and
// waits until `window.restarted()` is called.

import { tour } from "./tour-core.js";

const p = new URLSearchParams(location.hash.slice(1));
const out = document.getElementById("out") as HTMLElement;
const w = window as unknown as { restarted?: () => void; tourResult?: { ok: number; failed: number } };
const log = (l: string) => {
  console.log(l);
  out.textContent += `${l}\n`;
};
w.tourResult = await tour({
  url: p.get("url") ?? location.origin,
  vae: p.get("vae") ?? "acme",
  username: p.get("user") ?? "admin",
  password: p.get("password") ?? "",
  transport: p.get("transport") === "http" ? "http" : "ws",
  tenant: p.get("tenant") ?? undefined,
  seed: Number(p.get("seed") ?? 1) * 1000 + (p.get("transport") === "http" ? 500 : 0),
  log,
  restart:
    p.get("restart") === "1"
      ? () =>
          new Promise<void>((resolve) => {
            w.restarted = resolve;
            log("RESTART");
          })
      : undefined,
}).catch((e: unknown) => {
  log(`FAIL the tour stopped: ${String(e)}`);
  return { ok: 0, failed: 1 };
});
