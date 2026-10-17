// The tour in Node.js:
//
//   node build/examples/tour.js URL VAE USER PASSWORD (ws|http) [--restart]
//
// With --restart the tour prints RESTART and waits for a line on standard input while the
// node is restarted, then checks that the client reconnected and the subscription resumed.

import { createInterface } from "node:readline";
import { tour } from "./tour-core.js";

const a = process.argv.slice(2);
if (a.length < 5) {
  console.error("tour URL VAE USER PASSWORD (ws|http) [--restart]");
  process.exit(2);
}
const lines = createInterface({ input: process.stdin })[Symbol.asyncIterator]();
const r = await tour({
  url: a[0],
  vae: a[1],
  username: a[2],
  password: a[3],
  transport: a[4] === "http" ? "http" : "ws",
  seed: (process.pid % 100000) * 1000 + (a[4] === "http" ? 500 : 0),
  log: (l) => console.log(l),
  restart: a.includes("--restart")
    ? async () => {
        console.log("RESTART");
        await lines.next();
      }
    : undefined,
});
process.exit(r.failed === 0 ? 0 : 1);
