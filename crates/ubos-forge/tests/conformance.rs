//! The BPA vectors through the in-memory adapter (STD-CONF-001, VER-INSP-0001).
//!
//! This is the test body of the mutation score (EXIT-1-11, DEC-061): every vector of the
//! profiles the kernel claims must pass; the PostgreSQL run is `forge conform --backend
//! postgres`. The kernel claims BPA 1.0-rc1: of the vectors that 1.0-rc2 adds (DEC-064), it
//! runs those whose clauses the steps 11–14 have implemented so far (`RC2_CLAUSES`,
//! DEC-070) until step 11c claims the release's clauses; the vectors a later release
//! supersedes are skipped (DEC-067).

use std::path::{Path, PathBuf};
use ubos_dvm::Adapter;
use ubos_forge::runner::{Outcome, run_vector, superseded};

/// The counting allocator behind the memory probe of the metering vectors.
#[global_allocator]
static ALLOC: cap::Cap<std::alloc::System> = cap::Cap::new(std::alloc::System, usize::MAX);

/// The release the kernel claims (STD-CONF-004): its manifest lists the vectors to run.
const CLAIMED_RELEASE: &str = "1.0-rc1";

/// The release in implementation (PH-1 steps 11–14): a vector of it runs when every clause
/// it verifies is implemented (DEC-070: the governance sheets of step 11b, the rules and
/// the Core clauses of step 11c; DEC-071: the lifecycles of step 12b; DEC-073: the queries
/// of step 13a; DEC-075: the in-process UBTP binding of step 14a; DEC-079: the versioning
/// messages and introspection queries of step 18b; DEC-082: the process of a failed mutate
/// and the replay from the journal's snapshots of step 18d; DEC-083: the revert resolutions
/// and the timer effects of events subscriptions of step 18d-3a; DEC-084: the sheet checks
/// at merge and the sheets of the rule binding of step 18d-3b).
const NEXT_RELEASE: &str = "1.0-rc9";
const RC2_CLAUSES: &[&str] = &[
    "STD-LGS-001",
    "STD-LGS-002",
    "STD-LGS-010",
    "STD-LGS-011",
    "STD-LGS-020",
    "STD-LGS-021",
    "STD-LGS-022",
    "STD-LGS-023",
    "STD-LGS-030",
    "STD-LGS-031",
    "STD-CLS-003",
    "STD-CLS-031",
    "STD-BPU-003",
    "STD-ISA-035",
    "STD-ISA-054",
    // BPA 1.0-rc3 (step 12b): lifecycles, transitions, actions and port availability
    "STD-ISA-001",
    "STD-CLS-004",
    "STD-ISA-017",
    "STD-ISA-036",
    "STD-ISA-050",
    "STD-BPU-011",
    // BPA 1.0-rc2 (step 13a): the textual query form and search.query
    "STD-EXPR-010",
    "STD-ISA-080",
    // BPA 1.0-rc2 (step 14a): the in-process UBTP binding
    "STD-PROTO-001",
    "STD-PROTO-002",
    "STD-PROTO-010",
    "STD-PROTO-011",
    "STD-PROTO-012",
    "STD-PROTO-013",
    "STD-PROTO-020",
    // BPA 1.0-rc4 (step 18b): versioning messages and introspection queries
    "STD-PROTO-014",
    "STD-PROTO-015",
    // BPA 1.0-rc5 (step 18d): replay from the journal's snapshots (implemented in step 18b)
    "STD-CTX-012",
    // BPA 1.0-rc6 (step 18d-3a): revert resolutions, timer effects of events subscriptions
    "STD-VER-070",
    "STD-TIME-050",
    // BPA 1.0-rc7 (step 18d-3b): the sheets of the rule binding
    "STD-TIME-040",
    // BPA 1.0-rc9 (PH-2 step 3b, DEC-099): currencies, FX rates and accounts
    "STD-FND-028",
    "STD-FND-029",
    "STD-KIND-029",
    // BPA 1.0-rc9 (PH-2 step 3c, DEC-100): projections and read.projection
    "STD-TIME-021",
    "STD-ISA-037",
    // BPA 1.0-rc8 (PH-2 step 3d, DEC-101): periods, close and reopen, close artefacts
    "STD-KIND-025",
    "STD-KIND-026",
    "STD-VER-055",
];

/// The clauses a vector verifies.
fn clauses(v: &serde_json::Value) -> Vec<String> {
    v["clauses"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|c| c.as_str().map(str::to_string))
        .collect()
}

/// The vector identifiers of a release, from the `files` of its manifest.
fn released(standard: &Path, version: &str) -> std::collections::BTreeSet<String> {
    let text =
        std::fs::read_to_string(standard.join("release").join(version).join("manifest.json"))
            .expect("release manifest");
    let m: serde_json::Value = serde_json::from_str(&text).expect("manifest json");
    m["files"]
        .as_array()
        .expect("files")
        .iter()
        .filter_map(|f| f["path"].as_str())
        .filter(|p| p.starts_with("vectors/") && p.ends_with(".json"))
        .filter_map(|p| {
            p.rsplit('/')
                .next()
                .map(|n| n.trim_end_matches(".json").to_string())
        })
        .collect()
}

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
    let mut skipped = 0usize;
    let superseded = superseded(&dir);
    let claimed = released(&dir.join(".."), CLAIMED_RELEASE);
    let next = released(&dir.join(".."), NEXT_RELEASE);
    for p in &paths {
        let v: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(p).expect("read")).expect("json");
        // every vector of a claimed profile must pass outright: a "not applicable" outcome
        // (an unsupported operation, STD-CONF-005) is a failure here, since 1,227 of 1,227
        // pass (EXIT-1-02); a vector of an unclaimed profile is skipped
        // a vector outside the claimed release, or one a later release supersedes, is skipped
        let id = v["id"].as_str().unwrap_or_default();
        let implemented = next.contains(id)
            && clauses(&v)
                .iter()
                .all(|c| RC2_CLAUSES.contains(&c.as_str()));
        if (!claimed.contains(id) && !implemented) || superseded.contains(id) {
            skipped += 1;
            continue;
        }
        match run_vector(&mut adapter, &v, None) {
            Outcome::Pass => {}
            Outcome::Fail(d) => failures.push(format!("{} {d}", v["id"])),
            Outcome::NotApplicable(d) => failures.push(format!("{} not applicable: {d}", v["id"])),
        }
    }
    eprintln!(
        "conformance: {} vectors run, {skipped} skipped",
        paths.len() - skipped
    );
    assert!(
        failures.is_empty(),
        "{} of {} vectors fail ({skipped} outside the claimed release skipped):\n{}",
        failures.len(),
        paths.len(),
        failures.join("\n")
    );
}
