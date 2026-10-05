# Replay corpus (SUITE-DVM-REPLAY, VER-CONF-8500, EXIT-1-08)

`expected.json` holds the SHA-256 outcome hashes of the 10,000 processes that `ubos-replay`
generates from the seed in the file: 6,983 host programs and 3,017 L2 scripts over seeded
inputs (exact decimals, money, dates, time zones, Unicode texts, lists), each run on a fresh
in-memory VAE with a fixed clock and entropy seed. The corpus itself is not stored: it is the
generator's output for the seed, the same on every platform.

```sh
cargo build --release --locked -p ubos-dvm --bin ubos-replay
./target/release/ubos-replay check --expected verify/replay/expected.json --report replay.json
./target/release/ubos-replay record --expected verify/replay/expected.json   # after a change of the generator (DEC record)
```

A change of the generator or of the kernel's canonical behaviour changes the hashes; the
expected file is then re-recorded on the reference platform (Linux x86-64) under a decision
record, never silently. The nightly workflow checks the corpus on Linux x86-64, Linux ARM64
and macOS ARM64 (`verify/reports/exit-1-08/`).
