// Serve dist/ on http://127.0.0.1:PORT (default 7080). The node must list this origin in
// `server.cors_origins` (a `forge dev` node in the `dev` preset is reached the same way).

import { createServer } from "node:http";
import { readFile } from "node:fs/promises";
import { extname, join, normalize } from "node:path";

const root = join(import.meta.dirname, "..", "dist");
const port = Number(process.env.PORT ?? process.argv[2] ?? 7080);
const types = { ".html": "text/html", ".js": "text/javascript", ".map": "application/json", ".json": "application/json" };
createServer(async (req, res) => {
  let path = normalize(join(root, decodeURIComponent(new URL(req.url, "http://x").pathname)));
  if (!path.startsWith(root)) {
    res.writeHead(403).end();
    return;
  }
  if (path === root || path.endsWith("/")) path = join(root, "index.html");
  try {
    const body = await readFile(path);
    res.writeHead(200, { "content-type": types[extname(path)] ?? "application/octet-stream" });
    res.end(body);
  } catch {
    res.writeHead(404).end();
  }
}).listen(port, "127.0.0.1", () => console.log(`engineering UI on http://127.0.0.1:${port}`));
