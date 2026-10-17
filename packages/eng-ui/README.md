# @ubos/eng-ui

The engineering UI prototype of UBOS (PH-1 step 17f, DEC-078; WSP at PH-1b): a minimal web
shell used for the acceptance of PH-1b, superseded by the Workspace (`@ubos/workspace`). It
reaches the node through the TypeScript SDK only (`@ubos/sdk`). AGPL-3.0-only.

What it offers:
- Connection: URL, VAE, tenant, binding (WebSocket or HTTP), and credentials (`dev`, password, API key, bearer token).
- A branch, a transaction time (`time`) and a valid time (`asof`) for every read.
- The class tree of the model, and lists of a class with its subclasses, filtered by an L1 `where` and paged in one snapshot.
- Details of an object: fields typed by the model (inherited ones marked), version, evidence and valid segment, live updates through a subscription, and file downloads.
- Forms generated from the model, for creating and editing objects: text, enum, integer, exact decimal, boolean, date, money, file upload and JSON fields. Errors are shown with their code, HTTP status and details.
- The history of an object, and the object at any of its commits.
- Port calls with their parameters, and explanations (the decision of a port, the cascade of a property).

The model is the `generated/model.json` that `forge types` (or `forge dev --types`) writes for
a project: load it from a file or a URL. Without a model the UI still lists and shows any
class by name.

```sh
npm ci && npm run build            # dist/
npm run serve                      # http://127.0.0.1:7080
forge dev path/to/project          # a node on 127.0.0.1:7070 that allows that origin
```

A node not started by `forge dev` must list the UI's origin in `server.cors_origins`.

`test/e2e.mjs` walks through the UI in headless Chromium against a `forge dev` node of
`verify/sdk/fixture`:
1. connects and loads the model;
2. submits a generated form that breaks a validation, then corrects it;
3. edits the object and opens its history;
4. lists `Party` and finds the `Customer` in it;
5. calls a port;
6. uploads and downloads a file;
7. sees another client's change appear live.

```sh
node test/e2e.mjs --forge ../../target/debug/forge --server ../../target/debug/ubos-server [--binding http]
```
