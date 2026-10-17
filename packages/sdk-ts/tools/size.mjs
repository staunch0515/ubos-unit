// The bundle size of the browser core (DSN-SDK-017, BENCH): the minimal application of
// examples/minimal.ts and an application importing every export, each bundled for browsers,
// minified and gzipped; both must stay under 60 KB.

import { build } from "esbuild";
import { gzipSync } from "node:zlib";

const LIMIT = 60 * 1024;
const cases = {
  "minimal application (examples/minimal.ts)": { entryPoints: ["examples/minimal.ts"] },
  "every export (src/index.ts)": {
    stdin: { contents: 'export * from "./src/index.ts";', resolveDir: ".", loader: "ts" },
  },
};
let failed = 0;
for (const [name, entry] of Object.entries(cases)) {
  const r = await build({
    ...entry,
    bundle: true,
    minify: true,
    format: "esm",
    platform: "browser",
    target: "es2022",
    write: false,
  });
  const raw = r.outputFiles[0].contents;
  const gz = gzipSync(raw, { level: 9 }).length;
  const ok = gz < LIMIT;
  failed += ok ? 0 : 1;
  console.log(`${ok ? "ok  " : "FAIL"} ${name}: ${(raw.length / 1024).toFixed(1)} KB minified, ${(gz / 1024).toFixed(1)} KB gzipped (limit 60 KB)`);
}
process.exit(failed ? 1 : 0);
