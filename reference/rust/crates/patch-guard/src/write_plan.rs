mod apply;
mod validation;

use std::ops::Range;

use anyhow::Result;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecodedInstruction {
    /// Byte offset relative to the start of the Expected Write.
    pub offset: usize,
    /// Number of replacement bytes consumed by this instruction.
    pub len: usize,
    /// Canonical instruction text accepted by the project reassembler.
    pub canonical: String,
}

/// Project adapter for the declared ISA profile.
///
/// The core verifies source assembly, contiguous instruction coverage, and
/// canonical disassembly round-trip. The project must test profile-wide ISA
/// coverage separately.
pub trait MachineCodeVerifier {
    /// Assemble the declared source independently of the replacement bytes in the write plan.
    fn assemble_source(&self, check: &MachineCodeCheck<'_>) -> Result<Vec<u8>>;

    /// Decode the replacement under the ISA profile declared by the project.
    fn disassemble(&self, check: &MachineCodeCheck<'_>) -> Result<Vec<DecodedInstruction>>;

    /// Assemble the canonical instruction sequence returned by `disassemble`.
    fn assemble_decoded(
        &self,
        check: &MachineCodeCheck<'_>,
        instructions: &[DecodedInstruction],
    ) -> Result<Vec<u8>>;
}

#[derive(Debug, Clone, Default)]
pub struct WritePlan {
    pub resize: Option<ResizePlan>,
    pub regions: Vec<ImageRegion>,
    pub writes: Vec<ExpectedWrite>,
}

#[cfg(test)]
#[path = "write_plan_tests.rs"]
mod tests;
