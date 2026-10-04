//! The gate-1a scenarios of `verify/scenarios` in memory (VER-SCN-1000, VER-INSP-0001).

use std::path::Path;
use ubos_forge::check::Backend;
use ubos_forge::project::Project;

#[test]
fn every_scenario_test_passes_in_memory() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../verify/scenarios");
    let project = Project::load(&root).expect("project");
    let report = ubos_forge::test::run(&project, &Backend { database: None }).expect("run");
    let failed: Vec<String> = report
        .results
        .iter()
        .filter(|r| !r.pass)
        .map(|r| format!("{}: {:?}", r.name, r.failure))
        .collect();
    assert!(report.results.len() >= 20, "{} tests", report.results.len());
    assert!(failed.is_empty(), "failing tests:\n{}", failed.join("\n"));
}
