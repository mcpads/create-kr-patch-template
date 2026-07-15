use super::*;

fn artifact(id: &str, byte: char) -> ArtifactReport {
    ArtifactReport::from_bytes(id, &[byte as u8]).unwrap()
}

fn passed_report(target: ArtifactReport) -> RuntimeEvidenceReport {
    RuntimeEvidenceReport {
        schema_version: 1,
        scenario_id: "boot-and-open-dialog".to_owned(),
        artifact: target,
        outcome: RuntimeOutcome::Passed,
        evidence: vec![artifact("trace", 'b')],
    }
}

#[test]
fn pass_is_bound_to_the_exact_artifact_and_evidence() {
    let target = artifact("patched-image", 'a');
    assert!(require_runtime_pass(&target, &passed_report(target.clone())).is_ok());

    let other = artifact("patched-image", 'c');
    assert!(require_runtime_pass(&other, &passed_report(target.clone())).is_err());

    let mut missing = passed_report(target.clone());
    missing.evidence.clear();
    assert!(require_runtime_pass(&target, &missing).is_err());

    let mut failed = passed_report(target.clone());
    failed.outcome = RuntimeOutcome::Failed;
    assert!(require_runtime_pass(&target, &failed).is_err());
}
