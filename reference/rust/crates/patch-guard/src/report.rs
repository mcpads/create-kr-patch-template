use std::collections::BTreeSet;

use anyhow::{Result, ensure};
use serde::Serialize;

use crate::{
    build_mode::{BuildMode, ReadinessReport},
    source::sha256_hex,
    write_plan::{ResizeReport, WriteReport},
};

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ArtifactReport {
    pub id: String,
    pub len: usize,
    pub sha256: String,
}

impl ArtifactReport {
    pub fn from_bytes(id: impl Into<String>, bytes: &[u8]) -> Result<Self> {
        let report = Self {
            id: id.into(),
            len: bytes.len(),
            sha256: sha256_hex(bytes),
        };
        report.validate()?;
        Ok(report)
    }

    pub fn validate(&self) -> Result<()> {
        ensure!(!self.id.trim().is_empty(), "artifact id is empty");
        ensure!(self.len > 0, "artifact {} is empty", self.id);
        ensure!(
            self.sha256.len() == 64 && self.sha256.bytes().all(|byte| byte.is_ascii_hexdigit()),
            "artifact {} has an invalid SHA-256",
            self.id
        );
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct BuildReport {
    pub schema_version: u32,
    pub mode: BuildMode,
    pub release_candidate: bool,
    pub source_inputs: Vec<ArtifactReport>,
    pub authored_inputs: Vec<ArtifactReport>,
    pub output: ArtifactReport,
    pub readiness: ReadinessReport,
    pub product_steps: Vec<String>,
    pub resize: Option<ResizeReport>,
    pub writes: Vec<WriteReport>,
}

impl BuildReport {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.schema_version == 1,
            "unsupported build report schema version {}",
            self.schema_version
        );
        ensure!(
            !self.source_inputs.is_empty(),
            "build report has no source inputs"
        );
        ensure!(
            self.mode == self.readiness.mode,
            "build mode differs from readiness report"
        );
        ensure!(
            self.release_candidate == self.readiness.release_candidate,
            "release-candidate state differs from readiness report"
        );
        ensure!(
            !self.product_steps.is_empty(),
            "build report has no product steps"
        );

        let mut ids = BTreeSet::new();
        for artifact in self.source_inputs.iter().chain(&self.authored_inputs) {
            artifact.validate()?;
            ensure!(
                ids.insert(artifact.id.as_str()),
                "duplicate build input artifact id {}",
                artifact.id
            );
        }
        self.output.validate()?;
        ensure!(
            !ids.contains(self.output.id.as_str()),
            "build output shadows input artifact id {}",
            self.output.id
        );
        Ok(())
    }
}

#[cfg(test)]
#[path = "report_tests.rs"]
mod tests;
