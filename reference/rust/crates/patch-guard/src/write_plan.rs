use std::{collections::BTreeSet, ops::Range};

use anyhow::{Context, Result, bail, ensure};
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

struct ValidatedPlan {
    output_len: usize,
    write_ranges: Vec<Range<usize>>,
    region_indices: Vec<usize>,
}

impl WritePlan {
    pub fn apply(
        &self,
        baseline: &[u8],
        machine_code_verifier: Option<&dyn MachineCodeVerifier>,
    ) -> Result<ApplyResult> {
        let validated = self.validate(baseline, machine_code_verifier)?;
        let mut output = baseline[..baseline.len().min(validated.output_len)].to_vec();
        output.resize(validated.output_len, 0);
        for (write, range) in self.writes.iter().zip(&validated.write_ranges) {
            output[range.clone()].copy_from_slice(&write.replacement);
        }
        self.audit(baseline, &output, machine_code_verifier)?;

        let resize = self.resize.as_ref().map(|resize| ResizeReport {
            actor: resize.actor.clone(),
            purpose: resize.purpose.clone(),
            input_len: resize.expected_input_len,
            output_len: resize.output_len,
        });
        let writes = self
            .writes
            .iter()
            .zip(validated.region_indices)
            .map(|(write, region_index)| WriteReport {
                id: write.id.clone(),
                actor: write.actor.clone(),
                purpose: write.purpose.clone(),
                region_id: self.regions[region_index].id.clone(),
                intent: write.intent.label().to_owned(),
                offset: write.offset,
                len: write.replacement.len(),
                changed_bytes: changed_byte_count(baseline, write),
            })
            .collect();
        Ok(ApplyResult {
            output,
            resize,
            writes,
        })
    }

    pub fn audit(
        &self,
        baseline: &[u8],
        output: &[u8],
        machine_code_verifier: Option<&dyn MachineCodeVerifier>,
    ) -> Result<()> {
        let validated = self.validate(baseline, machine_code_verifier)?;
        ensure!(
            output.len() == validated.output_len,
            "output length differs from the validated write plan"
        );

        for (write, range) in self.writes.iter().zip(&validated.write_ranges) {
            ensure!(
                output[range.clone()] == write.replacement,
                "output does not contain the planned bytes for write {}",
                write.id
            );
        }
        for region in &self.regions {
            if region.kind == RegionKind::Protected {
                ensure!(
                    output[region.range.clone()] == baseline[region.range.clone()],
                    "protected region {} changed",
                    region.id
                );
            }
        }
        for offset in 0..baseline.len().min(output.len()) {
            if baseline[offset] != output[offset] {
                ensure!(
                    validated
                        .write_ranges
                        .iter()
                        .any(|range| range.contains(&offset)),
                    "untracked final diff at offset {offset:#X}"
                );
            }
        }
        Ok(())
    }

    fn validate(
        &self,
        baseline: &[u8],
        machine_code_verifier: Option<&dyn MachineCodeVerifier>,
    ) -> Result<ValidatedPlan> {
        let output_len = match &self.resize {
            Some(resize) => {
                ensure_nonempty("resize actor", &resize.actor)?;
                ensure_nonempty("resize purpose", &resize.purpose)?;
                ensure!(
                    resize.expected_input_len == baseline.len(),
                    "resize input length precondition failed"
                );
                ensure!(
                    resize.output_len != baseline.len(),
                    "resize plan does not change the image length"
                );
                resize.output_len
            }
            None => baseline.len(),
        };

        let mut region_ids = BTreeSet::new();
        for (index, region) in self.regions.iter().enumerate() {
            ensure_nonempty("region id", &region.id)?;
            ensure_nonempty("region reason", &region.reason)?;
            ensure!(
                region_ids.insert(region.id.as_str()),
                "duplicate region id {}",
                region.id
            );
            ensure!(
                region.range.start < region.range.end && region.range.end <= output_len,
                "region {} is outside the planned output",
                region.id
            );
            if region.kind == RegionKind::Protected {
                ensure!(
                    region.range.end <= baseline.len(),
                    "protected region {} is outside the original image",
                    region.id
                );
            }
            for other in self.regions.iter().take(index) {
                ensure!(
                    !intersects(&region.range, &other.range),
                    "region {} overlaps {}",
                    region.id,
                    other.id
                );
            }
        }

        let write_ranges = self.write_ranges()?;
        let mut write_ids = BTreeSet::new();
        let mut region_indices = Vec::with_capacity(self.writes.len());
        for (index, (write, range)) in self.writes.iter().zip(&write_ranges).enumerate() {
            ensure_nonempty("write id", &write.id)?;
            ensure_nonempty("write actor", &write.actor)?;
            ensure_nonempty("write purpose", &write.purpose)?;
            ensure!(
                write_ids.insert(write.id.as_str()),
                "duplicate Expected Write id {}",
                write.id
            );
            ensure!(
                range.end <= output_len,
                "Expected Write {} is outside the planned output",
                write.id
            );

            let overlap_len = range.end.min(baseline.len()).saturating_sub(write.offset);
            ensure!(
                write.expected_original.len() == overlap_len,
                "Expected Write {} has {} precondition bytes; {} required",
                write.id,
                write.expected_original.len(),
                overlap_len
            );
            ensure!(
                baseline
                    .get(write.offset..write.offset + overlap_len)
                    .is_some_and(|bytes| bytes == write.expected_original),
                "Expected Write {} original-byte precondition failed",
                write.id
            );

            for (other_index, other) in write_ranges.iter().enumerate().take(index) {
                ensure!(
                    !intersects(range, other),
                    "Expected Write {} by actor {} overlaps {} by actor {}",
                    write.id,
                    write.actor,
                    self.writes[other_index].id,
                    self.writes[other_index].actor
                );
            }

            let containing: Vec<usize> = self
                .regions
                .iter()
                .enumerate()
                .filter_map(|(region_index, region)| {
                    contains(&region.range, range).then_some(region_index)
                })
                .collect();
            ensure!(
                containing.len() == 1,
                "Expected Write {} is not contained in exactly one declared region",
                write.id
            );
            let region_index = containing[0];
            let region = &self.regions[region_index];
            ensure!(
                region.kind != RegionKind::Protected,
                "Expected Write {} intersects protected region {}",
                write.id,
                region.id
            );
            ensure!(
                write.intent.region_kind() == region.kind,
                "Expected Write {} intent {} does not match region {} kind {:?}",
                write.id,
                write.intent.label(),
                region.id,
                region.kind
            );

            if let WriteIntent::MachineCode(provenance) = &write.intent {
                ensure_nonempty("assembly source id", &provenance.assembly_source_id)?;
                ensure_nonempty("ISA profile id", &provenance.isa_profile_id)?;
                let Some(verifier) = machine_code_verifier else {
                    bail!(
                        "Expected Write {} targets machine code without a project verifier",
                        write.id
                    );
                };
                let check = MachineCodeCheck {
                    region,
                    write,
                    provenance,
                    baseline,
                };
                let assembled = verifier.assemble(&check)?;
                ensure!(
                    assembled == write.replacement,
                    "Expected Write {} replacement differs from assembled source {}",
                    write.id,
                    provenance.assembly_source_id
                );
                ensure!(
                    verifier.decoded_len(&check)? == write.replacement.len(),
                    "Expected Write {} does not decode completely under ISA profile {}",
                    write.id,
                    provenance.isa_profile_id
                );
            }
            region_indices.push(region_index);
        }

        if output_len > baseline.len() {
            for offset in baseline.len()..output_len {
                ensure!(
                    write_ranges.iter().any(|range| range.contains(&offset)),
                    "grown output byte {offset:#X} has no Expected Write actor"
                );
            }
        }

        Ok(ValidatedPlan {
            output_len,
            write_ranges,
            region_indices,
        })
    }

    fn write_ranges(&self) -> Result<Vec<Range<usize>>> {
        self.writes
            .iter()
            .map(|write| {
                ensure!(
                    !write.replacement.is_empty(),
                    "Expected Write {} is empty",
                    write.id
                );
                let end = write
                    .offset
                    .checked_add(write.replacement.len())
                    .with_context(|| format!("Expected Write {} range overflow", write.id))?;
                Ok(write.offset..end)
            })
            .collect()
    }
}

fn ensure_nonempty(label: &str, value: &str) -> Result<()> {
    ensure!(!value.trim().is_empty(), "{label} is empty");
    Ok(())
}

fn contains(outer: &Range<usize>, inner: &Range<usize>) -> bool {
    outer.start <= inner.start && inner.end <= outer.end
}

fn intersects(left: &Range<usize>, right: &Range<usize>) -> bool {
    left.start < right.end && right.start < left.end
}

fn changed_byte_count(baseline: &[u8], write: &ExpectedWrite) -> usize {
    write
        .replacement
        .iter()
        .enumerate()
        .filter(|(index, value)| {
            baseline
                .get(write.offset + index)
                .is_none_or(|original| original != *value)
        })
        .count()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn region(id: &str, range: Range<usize>, kind: RegionKind) -> ImageRegion {
        ImageRegion {
            id: id.to_owned(),
            range,
            kind,
            reason: "self-authored test region".to_owned(),
        }
    }

    fn write(
        id: &str,
        actor: &str,
        offset: usize,
        expected: &[u8],
        replacement: &[u8],
        intent: WriteIntent,
    ) -> ExpectedWrite {
        ExpectedWrite {
            id: id.to_owned(),
            actor: actor.to_owned(),
            purpose: "exercise write planning".to_owned(),
            offset,
            expected_original: expected.to_vec(),
            replacement: replacement.to_vec(),
            intent,
        }
    }

    struct ExactAssemblyVerifier;

    impl MachineCodeVerifier for ExactAssemblyVerifier {
        fn assemble(&self, check: &MachineCodeCheck<'_>) -> Result<Vec<u8>> {
            ensure!(
                check.provenance.assembly_source_id == "asm/hook.s",
                "unexpected assembly source"
            );
            ensure!(
                check.provenance.isa_profile_id == "fixture-isa-v1",
                "unexpected ISA profile"
            );
            Ok(vec![0xaa, 0xbb])
        }

        fn decoded_len(&self, check: &MachineCodeCheck<'_>) -> Result<usize> {
            ensure!(check.write.replacement == [0xaa, 0xbb]);
            Ok(2)
        }
    }

    #[test]
    fn accepts_owned_non_overlapping_data_write() {
        let plan = WritePlan {
            regions: vec![region("data", 1..3, RegionKind::Data)],
            writes: vec![write(
                "text",
                "text-layout",
                1,
                &[1, 2],
                &[8, 9],
                WriteIntent::Data,
            )],
            ..WritePlan::default()
        };
        assert_eq!(
            plan.apply(&[0, 1, 2, 3], None).unwrap().output,
            [0, 8, 9, 3]
        );
    }

    #[test]
    fn rejects_overlap_even_for_the_same_actor() {
        let plan = WritePlan {
            regions: vec![region("data", 0..4, RegionKind::Data)],
            writes: vec![
                write("first", "layout", 1, &[1, 2], &[8, 9], WriteIntent::Data),
                write("second", "layout", 2, &[2], &[7], WriteIntent::Data),
            ],
            ..WritePlan::default()
        };
        assert!(plan.apply(&[0, 1, 2, 3], None).is_err());
    }

    #[test]
    fn rejects_raw_data_intent_in_machine_code_region() {
        let plan = WritePlan {
            regions: vec![region("code", 0..2, RegionKind::MachineCode)],
            writes: vec![write(
                "raw-opcodes",
                "hook",
                0,
                &[0, 1],
                &[0xaa, 0xbb],
                WriteIntent::Data,
            )],
            ..WritePlan::default()
        };
        assert!(plan.apply(&[0, 1], None).is_err());
    }

    #[test]
    fn machine_code_requires_and_uses_project_verifier() {
        let plan = WritePlan {
            regions: vec![region("code", 0..2, RegionKind::MachineCode)],
            writes: vec![write(
                "assembled-hook",
                "hook",
                0,
                &[0, 1],
                &[0xaa, 0xbb],
                WriteIntent::MachineCode(MachineCodeProvenance {
                    assembly_source_id: "asm/hook.s".to_owned(),
                    isa_profile_id: "fixture-isa-v1".to_owned(),
                }),
            )],
            ..WritePlan::default()
        };
        assert!(plan.apply(&[0, 1], None).is_err());
        assert_eq!(
            plan.apply(&[0, 1], Some(&ExactAssemblyVerifier))
                .unwrap()
                .output,
            [0xaa, 0xbb]
        );
    }

    #[test]
    fn growth_and_final_diff_require_explicit_ownership() {
        let baseline = [0, 1];
        let resize = ResizePlan {
            actor: "layout".to_owned(),
            purpose: "append payload".to_owned(),
            expected_input_len: baseline.len(),
            output_len: 4,
        };
        let incomplete = WritePlan {
            resize: Some(resize.clone()),
            regions: vec![region("tail", 2..4, RegionKind::Data)],
            writes: vec![write("tail", "layout", 2, &[], &[2], WriteIntent::Data)],
        };
        assert!(incomplete.apply(&baseline, None).is_err());

        let complete = WritePlan {
            resize: Some(resize),
            regions: vec![
                region("metadata", 0..1, RegionKind::Metadata),
                region("tail", 2..4, RegionKind::Data),
            ],
            writes: vec![write("tail", "layout", 2, &[], &[2, 3], WriteIntent::Data)],
        };
        let mut output = complete.apply(&baseline, None).unwrap().output;
        output[0] = 9;
        assert!(complete.audit(&baseline, &output, None).is_err());
    }
}
