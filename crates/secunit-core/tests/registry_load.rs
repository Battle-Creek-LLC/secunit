//! End-to-end load tests against the multi-system fixture org.

use std::path::PathBuf;

use secunit_core::registry::loader;

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join("testdata/orgs")
        .join(name)
        .canonicalize()
        .expect("fixture dir must exist")
}

#[test]
fn loads_multi_system_fixture() {
    let (reg, report) = loader::load(&fixture("multi-system"));
    assert!(report.errors.is_empty(), "load errors: {:?}", report.errors);
    assert!(reg.controls.contains_key("sca-weekly-dependency-scan"));
    assert!(reg.controls.contains_key("aa-weekly-audit-review"));
    assert!(reg.controls.contains_key("ca-quarterly-vuln-scan"));
    assert!(!reg.inventory.entries("source_repos").is_empty());
    assert!(!reg.inventory.entries("cloud_accounts").is_empty());
    assert_eq!(reg.schedule.overrides.len(), 5);
    assert!(reg.state.controls.contains_key("aa-weekly-audit-review"));
}
