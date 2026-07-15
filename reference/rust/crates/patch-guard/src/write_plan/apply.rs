use anyhow::{Result, ensure};

use super::{
    ApplyResult, ExpectedWrite, MachineCodeVerifier, RegionKind, ResizeReport, WritePlan,
    WriteReport,
};

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
