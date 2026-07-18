use std::{collections::BTreeSet, ops::Range};

use anyhow::{Context, Result, bail, ensure};

use super::{MachineCodeCheck, MachineCodeVerifier, RegionKind, WriteIntent, WritePlan};

pub(super) struct ValidatedPlan {
    pub(super) output_len: usize,
    pub(super) write_ranges: Vec<Range<usize>>,
}

impl WritePlan {
    pub(super) fn validate(
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
