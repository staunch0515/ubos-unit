// The minimal browser application of DSN-SDK-017: connect, read one object, follow it.

import { connect } from "../src/index.js";

const c = await connect({ url: location.origin, vae: "acme", auth: { kind: "api_key", key: "…" } });
const item = await c.get("ubos://acme/Customer/acme");
document.body.textContent = `${item.slug} ${item.version}`;
await c.subscribe(`ubos://acme/Customer/${item.slug}`, (u) => {
  if (u.kind === "change") document.body.textContent = JSON.stringify(u.body);
});
