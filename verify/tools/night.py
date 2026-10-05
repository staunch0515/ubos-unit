#!/usr/bin/env python3
"""Append one night's row to the night log (VER-SIM-0001, EXIT-1-05; PH-1 step 8c, DEC-061).

    night.py <results dir> <nights.md>

The results directory holds the artifacts of the nightly workflow: simulation/simulation.json,
properties/properties.json, replay-*/replay-*.json. A night is clean when the simulation ran
10,000 seeds with 0 violations; the row also carries the properties and the replay results."""
import datetime
import glob
import json
import os
import sys


def load(path):
    try:
        return json.load(open(path))
    except (OSError, ValueError):
        return None


def main(results, log):
    sim = load(os.path.join(results, 'simulation', 'simulation.json'))
    props = load(os.path.join(results, 'properties', 'properties.json'))
    replays = [load(p) for p in sorted(glob.glob(os.path.join(results, 'replay-*', 'replay-*.json')))]
    today = datetime.date.today().isoformat()
    if sim:
        seeds = f"{sim['seeds']['from']}–{sim['seeds']['to']}"
        viol = len(sim['violations'])
        clean = 'clean' if viol == 0 and sim['seeds']['count'] >= 10000 else 'NOT clean'
    else:
        seeds, viol, clean = '—', '—', 'no run'
    if props:
        failures = sum(len(r['failures']) for r in props['results'])
        prop_text = f"{len(props['results'])} properties, {failures} failures"
    else:
        prop_text = 'no run'
    rep = []
    for r in replays:
        if not r:
            continue
        p = r['platform']
        rep.append(f"{p['os']}-{p['arch']}: {r['identical']}/{r['expected']['count']}, replays {r['replays']['identical']}/{r['replays']['sampled']}")
    rep_text = '; '.join(rep) if rep else 'no run'
    rows = open(log).read().rstrip('\n').split('\n') if os.path.exists(log) else []
    # the streak: consecutive clean nights ending with this one
    streak = 1 if clean == 'clean' else 0
    for line in reversed(rows):
        if not line.startswith('| 20'):
            continue
        cells = [c.strip() for c in line.strip('|').split('|')]
        if clean == 'clean' and len(cells) > 3 and cells[3] == 'clean':
            streak += 1
        else:
            break
    rows.append(f"| {today} | {seeds} | {viol} | {clean} | {streak} | {prop_text} | {rep_text} |")
    open(log, 'w').write('\n'.join(rows) + '\n')
    print(rows[-1])


if __name__ == '__main__':
    main(sys.argv[1], sys.argv[2])
