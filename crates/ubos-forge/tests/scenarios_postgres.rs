//! The gate-1a scenarios of `verify/scenarios` on PostgreSQL (VER-SCN-1000, VER-INSP-0001).
//!
//! Runs when `UBOS_TEST_DATABASE` names a PostgreSQL connection string (CI and the mutation
//! runs set it), so that the storage adapter is part of the test bodies; skipped otherwise.

use std::path::Path;
use ubos_forge::check::Backend;
use ubos_forge::project::Project;

#[test]
fn every_scenario_test_passes_on_postgres() {
    let Ok(database) = std::env::var("UBOS_TEST_DATABASE") else {
        eprintln!("UBOS_TEST_DATABASE not set: skipped");
        return;
    };
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../verify/scenarios");
    let project = Project::load(&root).expect("project");
    let backend = Backend {
        database: Some(database),
    };
    let report = ubos_forge::test::run(&project, &backend).expect("run");
    let failed: Vec<String> = report
        .results
        .iter()
        .filter(|r| !r.pass)
        .map(|r| format!("{}: {:?}", r.name, r.failure))
        .collect();
    assert!(report.results.len() >= 20, "{} tests", report.results.len());
    assert!(failed.is_empty(), "failing tests:\n{}", failed.join("\n"));
}
