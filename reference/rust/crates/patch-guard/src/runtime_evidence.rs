use std::collections::BTreeSet;

use anyhow::{Result, ensure};
use serde::Serialize;

use crate::report::ArtifactReport;

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeOutcome {
    Passed,
    Failed,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct RuntimeEvidenceReport {
    pub schema_version: u32,
    pub scenario_id: String,
    pub artifact: ArtifactReport,
    pub outcome: RuntimeOutcome,
    pub evidence: Vec<ArtifactReport>,
}

pub fn require_runtime_pass(
    expected_artifact: &ArtifactReport,
    report: &RuntimeEvidenceReport,
) -> Result<()> {
    ensure!(
        report.schema_version == 1,
        "unsupported runtime evidence schema version {}",
        report.schema_version
    );
    ensure!(
        !report.scenario_id.trim().is_empty(),
        "runtime scenario id is empty"
    );
    validate_artifact("expected runtime artifact", expected_artifact)?;
    validate_artifact("runtime artifact", &report.artifact)?;
    ensure!(
        &report.artifact == expected_artifact,
        "runtime evidence targets a different build artifact"
    );
    ensure!(
        report.outcome == RuntimeOutcome::Passed,
        "runtime scenario {} did not pass",
        report.scenario_id
    );
    ensure!(
        !report.evidence.is_empty(),
        "passed runtime scenario {} has no evidence artifacts",
        report.scenario_id
    );

    let mut ids = BTreeSet::new();
    for artifact in &report.evidence {
        validate_artifact("runtime evidence artifact", artifact)?;
        ensure!(
            ids.insert(artifact.id.as_str()),
            "duplicate runtime evidence artifact id {}",
            artifact.id
        );
    }
    Ok(())
}

fn validate_artifact(label: &str, artifact: &ArtifactReport) -> Result<()> {
    artifact
        .validate()
        .map_err(|error| anyhow::anyhow!("{label}: {error}"))
}

#[cfg(test)]
mod tests {
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
}
