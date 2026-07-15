mod support;

use anyhow::Result;
use patch_guard::{ArtifactReport, RuntimeEvidenceReport, RuntimeOutcome, require_runtime_pass};

use support::run_manifest;

#[test]
fn runtime_evidence_cases_match_language_neutral_expectations() {
    run_manifest("runtime-evidence.json", run_runtime_evidence_scenario);
}

fn run_runtime_evidence_scenario(scenario: &str) -> Result<()> {
    let expected = artifact("patched-image", 'a');
    let mut report = RuntimeEvidenceReport {
        schema_version: 1,
        scenario_id: "boot-and-open-dialog".to_owned(),
        artifact: expected.clone(),
        outcome: RuntimeOutcome::Passed,
        evidence: vec![artifact("trace", 'b')],
    };
    match scenario {
        "passed_evidence_for_exact_artifact" => {}
        "evidence_from_different_build" => report.artifact = artifact("patched-image", 'c'),
        "passed_without_evidence" => report.evidence.clear(),
        "failed_runtime_scenario" => report.outcome = RuntimeOutcome::Failed,
        other => panic!("unknown runtime-evidence conformance scenario {other}"),
    }
    require_runtime_pass(&expected, &report)
}

fn artifact(id: &str, hex_digit: char) -> ArtifactReport {
    ArtifactReport::from_bytes(id, &[hex_digit as u8]).unwrap()
}
