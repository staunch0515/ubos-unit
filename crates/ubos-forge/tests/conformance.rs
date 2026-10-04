//! The BPA 1.0-rc1 vectors through the in-memory adapter (STD-CONF-001, VER-INSP-0001).
//!
//! This is the test body of the mutation score (EXIT-1-11, DEC-061): every vector of
//! `standard/vectors` must pass; the PostgreSQL run is `forge conform --backend postgres`.

use std::path::{Path, PathBuf};
use ubos_dvm::Adapter;
use ubos_forge::runner::{Outcome, run_vector};

/// The counting allocator behind the memory probe of the metering vectors.
#[global_allocator]
static ALLOC: cap::Cap<std::alloc::System> = cap::Cap::new(std::alloc::System, usize::MAX);

fn files(dir: &Path, out: &mut Vec<PathBuf>) {
    for e in std::fs::read_dir(dir).expect("vectors directory") {
        let p = e.expect("entry").path();
        if p.is_dir() {
            files(&p, out);
        } else if p.extension().is_some_and(|x| x == "json") {
            out.push(p);
        }
    }
}

#[test]
fn every_vector_passes_in_memory() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../standard/vectors");
    let mut paths = Vec::new();
    files(&dir, &mut paths);
    paths.sort();
    assert!(paths.len() >= 1_000, "{} vectors found", paths.len());
    ubos_dvm::memory::install_probe(|| ALLOC.allocated());
    let mut adapter = Adapter::new().expect("adapter");
    let mut failures = Vec::new();
    for p in &paths {
        let v: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(p).expect("read")).expect("json");
        // every vector must pass outright: a "not applicable" outcome (an unsupported
        // operation, STD-CONF-005) is a failure here, since 1,227 of 1,227 pass (EXIT-1-02)
        match run_vector(&mut adapter, &v, None) {
            Outcome::Pass => {}
            Outcome::Fail(d) => failures.push(format!("{} {d}", v["id"])),
            Outcome::NotApplicable(d) => failures.push(format!("{} not applicable: {d}", v["id"])),
        }
    }
    assert!(
        failures.is_empty(),
        "{} of {} vectors fail:\n{}",
        failures.len(),
        paths.len(),
        failures.join("\n")
    );
}
