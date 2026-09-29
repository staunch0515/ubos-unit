#!/usr/bin/env python3
"""Check the dependency graph of the implementation against the specification (AR-031, DEC-028).

The allowed graph is read from the crate table of UBS-ARC-07 §2
(docs/50-system/20-architecture/07-codebase-and-delivery.md), so the specification stays the
single source. Checks:
  * every crate under crates/ is listed in the table;
  * every internal dependency (a `ubos-*` crate) of a crate under crates/ is allowed by its row;
  * a Cargo manifest under verify/ depends only on ubos-types, ubos-crypto and ubos-proof;
  * nothing under standard/ depends on any internal crate.

Usage: python3 docs/tools/check_deps.py
"""
import os
import re
import sys
import tomllib

REPO = os.path.abspath(os.path.join(os.path.dirname(os.path.abspath(__file__)), '..', '..'))
ARC07 = os.path.join(REPO, 'docs', '50-system', '20-architecture', '07-codebase-and-delivery.md')
VERIFY_ALLOWED = {'ubos-types', 'ubos-crypto', 'ubos-proof'}
DEP_TABLES = ('dependencies', 'dev-dependencies', 'build-dependencies')


def allowed_graph():
    text = open(ARC07, encoding='utf-8').read()
    section = text.split('## 2. Rust crates', 1)[1].split('\n## ', 1)[0]
    layer_of, raw = {}, {}
    for line in section.splitlines():
        m = re.match(r'^\| `([a-z0-9-]+)` \| (\w+) \| .*? \| (.*) \|$', line)
        if m:
            name, layer, deps = m.groups()
            layer_of[name] = layer
            raw[name] = deps
    order = list(raw)
    graph = {}
    for name in order:
        deps = raw[name]
        allowed = set(re.findall(r'`(ubos-[a-z0-9-]+)`', deps))
        if 'all engine crates above' in deps:
            allowed |= {c for c in order[:order.index(name)] if layer_of[c] == 'engine'}
        elif 'engine crates' in deps:
            allowed |= {c for c in order if layer_of[c] == 'engine' and c != name}
        graph[name] = allowed
    return graph


def internal_deps(manifest):
    data = tomllib.load(open(manifest, 'rb'))
    tables = [data.get(t, {}) for t in DEP_TABLES]
    for target in data.get('target', {}).values():
        tables += [target.get(t, {}) for t in DEP_TABLES]
    deps = set()
    for table in tables:
        for key, spec in table.items():
            name = spec.get('package', key) if isinstance(spec, dict) else key
            if name.startswith('ubos-'):
                deps.add(name)
    return data.get('package', {}).get('name'), deps


def main():
    graph = allowed_graph()
    errors, checked = [], 0
    for top in ('crates', 'verify', 'standard'):
        for root, dirs, files in os.walk(os.path.join(REPO, top)):
            dirs[:] = [d for d in dirs if d != 'target']
            if 'Cargo.toml' not in files:
                continue
            manifest = os.path.join(root, 'Cargo.toml')
            rel = os.path.relpath(manifest, REPO)
            name, deps = internal_deps(manifest)
            checked += 1
            if top == 'crates':
                if name not in graph:
                    errors.append('%s: crate %s is not listed in UBS-ARC-07 §2' % (rel, name))
                    continue
                allowed = graph[name]
            elif top == 'verify':
                allowed = VERIFY_ALLOWED
            else:
                allowed = set()
            for d in sorted(deps - allowed):
                errors.append('%s: %s may not depend on %s (AR-031, DEC-028)' % (rel, name, d))
    print('dependency graph: crates in UBS-ARC-07 %d, manifests checked %d' % (len(graph), checked))
    for e in errors:
        print('ERROR', e)
    print('errors: %d' % len(errors))
    return 1 if errors else 0


if __name__ == '__main__':
    sys.exit(main())
