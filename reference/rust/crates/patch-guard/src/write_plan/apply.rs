use anyhow::{Result, ensure};

use super::{MachineCodeVerifier, RegionKind, WritePlan};

impl WritePlan {
    pub fn apply(
        &self,
        baseline: &[u8],
        machine_code_verifier: Option<&dyn MachineCodeVerifier>,
    ) -> Result<Vec<u8>> {
        let validated = self.validate(baseline, machine_code_verifier)?;
        let mut output = baseline[..baseline.len().min(validated.output_len)].to_vec();
        output.resize(validated.output_len, 0);
        for (write, range) in self.writes.iter().zip(&validated.write_ranges) {
            output[range.clone()].copy_from_slice(&write.replacement);
        }
        self.audit(baseline, &output, machine_code_verifier)?;
        Ok(output)
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
