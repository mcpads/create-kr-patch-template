use crate::build_mode::{BuildMode, ReadinessReport};

use super::*;

#[test]
fn artifact_identity_is_derived_from_bytes() {
    let report = ArtifactReport::from_bytes("fixture", b"data").unwrap();
    assert_eq!(report.len, 4);
    assert_eq!(report.sha256, sha256_hex(b"data"));
}

#[test]
fn build_report_rejects_missing_or_duplicate_inputs() {
    let readiness = ReadinessReport {
        scope_id: "scope".to_owned(),
        mode: BuildMode::Development,
        release_candidate: false,
        population_status: crate::build_mode::PopulationStatus::Confirmed,
        known_population_units: 1,
        review_revision: "review-revision".to_owned(),
        localized_units: 1,
        source_preserved_units: 0,
        unresolved_units: Vec::new(),
    };
    let input = ArtifactReport::from_bytes("input", b"source").unwrap();
    let mut report = BuildReport {
        schema_version: 1,
        mode: BuildMode::Development,
        release_candidate: false,
        source_inputs: vec![input.clone()],
        authored_inputs: Vec::new(),
        output: ArtifactReport::from_bytes("output", b"result").unwrap(),
        readiness,
        product_steps: vec!["build".to_owned()],
        resize: None,
        writes: Vec::new(),
    };
    assert!(report.validate().is_ok());

    report.authored_inputs.push(input);
    assert!(report.validate().is_err());
    report.authored_inputs.clear();
    report.source_inputs.clear();
    assert!(report.validate().is_err());
}
