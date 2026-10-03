//! Memory accounted per execution (DSN-DVM-612, W-2). The host binary installs a
//! counting global allocator (the `cap` crate) and registers a probe of the live bytes;
//! the L2 host records the probe at the start of an execution and checks the growth at
//! every metering point. One execution runs at a time in the adapter and the suites, so
//! the process-wide count is the execution's count; a multi-worker host of step 7 runs
//! one execution per worker process or accounts per worker (DEC-058). Without a probe no
//! memory limit is enforced.

use std::sync::OnceLock;

static PROBE: OnceLock<fn() -> usize> = OnceLock::new();

/// Register the probe of live bytes (once per process).
pub fn install_probe(probe: fn() -> usize) {
    let _ = PROBE.set(probe);
}

/// Live bytes, when a probe is installed.
pub fn current() -> Option<usize> {
    PROBE.get().map(|p| p())
}
