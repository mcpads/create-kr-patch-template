mod apply;
mod validation;

use std::ops::Range;

use anyhow::Result;
use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RegionKind {
    Data,
    Metadata,
    MachineCode,
    Protected,
}

#[derive(Debug, Clone)]
pub struct ImageRegion {
    pub id: String,
    pub range: Range<usize>,
    pub kind: RegionKind,
    pub reason: String,
}

#[derive(Debug, Clone)]
pub struct ResizePlan {
    pub actor: String,
    pub purpose: String,
    pub expected_input_len: usize,
    pub output_len: usize,
}

#[derive(Debug, Clone)]
pub struct MachineCodeProvenance {
    pub assembly_source_id: String,
    pub isa_profile_id: String,
}

#[derive(Debug, Clone)]
pub enum WriteIntent {
    Data,
    Metadata,
    MachineCode(MachineCodeProvenance),
}

impl WriteIntent {
    fn region_kind(&self) -> RegionKind {
        match self {
            Self::Data => RegionKind::Data,
            Self::Metadata => RegionKind::Metadata,
            Self::MachineCode(_) => RegionKind::MachineCode,
        }
    }

    fn label(&self) -> &'static str {
        match self {
            Self::Data => "data",
            Self::Metadata => "metadata",
            Self::MachineCode(_) => "machine_code",
        }
    }
}

#[derive(Debug, Clone)]
pub struct ExpectedWrite {
    pub id: String,
    pub actor: String,
    pub purpose: String,
    pub offset: usize,
    pub expected_original: Vec<u8>,
    pub replacement: Vec<u8>,
    pub intent: WriteIntent,
}

pub struct MachineCodeCheck<'a> {
    pub region: &'a ImageRegion,
    pub write: &'a ExpectedWrite,
    pub provenance: &'a MachineCodeProvenance,
    pub baseline: &'a [u8],
}

pub trait MachineCodeVerifier {
    /// Assemble the declared source independently of the replacement bytes in the write plan.
    fn assemble(&self, check: &MachineCodeCheck<'_>) -> Result<Vec<u8>>;

    /// Return the number of consecutive replacement bytes decoded under the complete ISA profile.
    fn decoded_len(&self, check: &MachineCodeCheck<'_>) -> Result<usize>;
}

#[derive(Debug, Clone, Default)]
pub struct WritePlan {
    pub resize: Option<ResizePlan>,
    pub regions: Vec<ImageRegion>,
    pub writes: Vec<ExpectedWrite>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ResizeReport {
    pub actor: String,
    pub purpose: String,
    pub input_len: usize,
    pub output_len: usize,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct WriteReport {
    pub id: String,
    pub actor: String,
    pub purpose: String,
    pub region_id: String,
    pub intent: String,
    pub offset: usize,
    pub len: usize,
    pub changed_bytes: usize,
}

#[derive(Debug, Clone)]
pub struct ApplyResult {
    pub output: Vec<u8>,
    pub resize: Option<ResizeReport>,
    pub writes: Vec<WriteReport>,
}

#[cfg(test)]
#[path = "write_plan_tests.rs"]
mod tests;
