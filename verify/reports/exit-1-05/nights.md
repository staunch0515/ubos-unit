# Night log — consecutive clean simulation nights (VER-SIM-0001, EXIT-1-05)

One row per night, appended by the nightly workflow (`.github/workflows/nightly.yml`,
`verify/tools/night.py`) on the branch `nightly-log` and merged into `main` by the next
session. A night is clean when `ubos-sim` ran 10,000 seeds (the seed range moves with the
day) with 0 violations; the streak counts the consecutive clean nights ending with the row.
EXIT-1-05 needs a streak of 30. The properties column is the long property run of the night
and the replay column the corpus check on the three platforms (EXIT-1-08), which runs on
Sundays from 2026-10-14 on (the mutation shard too), within the Actions budget.

| Night (UTC) | Seeds | Violations | Verdict | Streak | Properties (100,000 cases each) | Replay corpus |
|---|---|---|---|---|---|---|
| 2026-10-06 | 207320000–207329999 | 0 | clean | 1 | 9 properties, 0 failures | macos-aarch64: 10000/10000, replays 1000/1000; linux-aarch64: 10000/10000, replays 1000/1000; linux-x86_64: 10000/10000, replays 1000/1000 |
| 2026-10-07 | 207330000–207339999 | 0 | clean | 2 | 9 properties, 0 failures | macos-aarch64: 10000/10000, replays 1000/1000; linux-aarch64: 10000/10000, replays 1000/1000; linux-x86_64: 10000/10000, replays 1000/1000 |
| 2026-10-15 | 207410000–207419999 | 0 | clean | 3 | 9 properties, 0 failures | no run |
| 2026-10-16 | 207420000–207429999 | 0 | clean | 4 | 10 properties, 0 failures | no run |
| 2026-10-17 | 207430000–207439999 | 0 | clean | 5 | 10 properties, 0 failures | no run |
| 2026-10-18 | 207440000–207449999 | 0 | clean | 6 | 10 properties, 0 failures | macos-aarch64: 10000/10000, replays 1000/1000; linux-aarch64: 10000/10000, replays 1000/1000; linux-x86_64: 10000/10000, replays 1000/1000 |
| 2026-10-19 | 207450000–207459999 | 0 | clean | 7 | 10 properties, 0 failures | no run |
| 2026-10-20 | 207460000–207469999 | 0 | clean | 8 | 10 properties, 0 failures | no run |
| 2026-10-21 | 207470000–207479999 | 0 | clean | 9 | 10 properties, 0 failures | no run |
| 2026-10-22 | 207480000–207489999 | 0 | clean | 10 | 10 properties, 0 failures | no run |
| 2026-10-23 | 207490000–207499999 | 0 | clean | 11 | 10 properties, 0 failures | no run |
| 2026-10-24 | 207500000–207509999 | 0 | clean | 12 | 10 properties, 0 failures | no run |
| 2026-10-25 | 207510000–207519999 | 0 | clean | 13 | 10 properties, 0 failures | macos-aarch64: 10000/10000, replays 1000/1000; linux-aarch64: 10000/10000, replays 1000/1000; linux-x86_64: 10000/10000, replays 1000/1000 |
