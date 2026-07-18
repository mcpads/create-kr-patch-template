mod support;

use std::ops::Range;

use anyhow::{Result, ensure};
use patch_guard::{
    DecodedInstruction, ExpectedWrite, ImageRegion, MachineCodeCheck, MachineCodeProvenance,
    MachineCodeVerifier, RegionKind, WriteIntent, WritePlan,
};

use support::run_manifest;

#[test]
fn write_plan_cases_match_language_neutral_expectations() {
    run_manifest("write-plan.json", run_write_scenario);
}

fn run_write_scenario(scenario: &str) -> Result<()> {
    let baseline = [0_u8, 1, 2, 3];
    match scenario {
        "owned_data_write" => data_plan().apply(&baseline, None).map(|_| ()),
        "overlapping_writes" => {
            let mut plan = data_plan();
            plan.regions[0].range = 0..4;
            plan.writes
                .push(write("second", "layout", 2, &[2], &[7], WriteIntent::Data));
            plan.apply(&baseline, None).map(|_| ())
        }
        "protected_region_write" => {
            let mut plan = data_plan();
            plan.regions[0].kind = RegionKind::Protected;
            plan.apply(&baseline, None).map(|_| ())
        }
        "wrong_original_bytes" => {
            let mut plan = data_plan();
            plan.writes[0].expected_original = vec![9, 9];
            plan.apply(&baseline, None).map(|_| ())
        }
        "untracked_final_diff" => {
            let plan = data_plan();
            let mut output = plan.apply(&baseline, None)?;
            output[3] = 9;
            plan.audit(&baseline, &output, None)
        }
        "raw_data_in_machine_code" => {
            let mut plan = machine_code_plan();
            plan.writes[0].intent = WriteIntent::Data;
            plan.apply(&baseline, None).map(|_| ())
        }
        "machine_code_without_verifier" => machine_code_plan().apply(&baseline, None).map(|_| ()),
        "verified_machine_code" => machine_code_plan()
            .apply(&baseline, Some(&FixtureIsaVerifier::exact()))
            .map(|_| ()),
        "machine_code_decode_gap" => machine_code_plan()
            .apply(&baseline, Some(&FixtureIsaVerifier::with_gap()))
            .map(|_| ()),
        "machine_code_reassembly_mismatch" => machine_code_plan()
            .apply(
                &baseline,
                Some(&FixtureIsaVerifier::with_reassembly_mismatch()),
            )
            .map(|_| ()),
        other => panic!("unknown write-plan conformance scenario {other}"),
    }
}

fn data_plan() -> WritePlan {
    WritePlan {
        regions: vec![region("data", 1..3, RegionKind::Data)],
        writes: vec![write(
            "first",
            "layout",
            1,
            &[1, 2],
            &[8, 9],
            WriteIntent::Data,
        )],
        ..WritePlan::default()
    }
}

fn machine_code_plan() -> WritePlan {
    WritePlan {
        regions: vec![region("code", 1..3, RegionKind::MachineCode)],
        writes: vec![write(
            "hook",
            "code-patch",
            1,
            &[1, 2],
            &[0xaa, 0xbb],
            WriteIntent::MachineCode(MachineCodeProvenance {
                assembly_source_id: "asm/hook.s".to_owned(),
                isa_profile_id: "fixture-isa-v1".to_owned(),
            }),
        )],
        ..WritePlan::default()
    }
}

struct FixtureIsaVerifier {
    decode_gap: bool,
    reassembly_mismatch: bool,
}

impl FixtureIsaVerifier {
    fn exact() -> Self {
        Self {
            decode_gap: false,
            reassembly_mismatch: false,
        }
    }

    fn with_gap() -> Self {
        Self {
            decode_gap: true,
            reassembly_mismatch: false,
        }
    }

    fn with_reassembly_mismatch() -> Self {
        Self {
            decode_gap: false,
            reassembly_mismatch: true,
        }
    }
}

impl MachineCodeVerifier for FixtureIsaVerifier {
    fn assemble_source(&self, check: &MachineCodeCheck<'_>) -> Result<Vec<u8>> {
        ensure!(check.provenance.assembly_source_id == "asm/hook.s");
        ensure!(check.provenance.isa_profile_id == "fixture-isa-v1");
        Ok(vec![0xaa, 0xbb])
    }

    fn disassemble(&self, check: &MachineCodeCheck<'_>) -> Result<Vec<DecodedInstruction>> {
        ensure!(check.write.replacement == [0xaa, 0xbb]);
        Ok(vec![
            DecodedInstruction {
                offset: 0,
                len: 1,
                canonical: "fixture_a".to_owned(),
            },
            DecodedInstruction {
                offset: usize::from(self.decode_gap) + 1,
                len: 1,
                canonical: "fixture_b".to_owned(),
            },
        ])
    }

    fn assemble_decoded(
        &self,
        _check: &MachineCodeCheck<'_>,
        instructions: &[DecodedInstruction],
    ) -> Result<Vec<u8>> {
        let mut output: Vec<u8> = instructions
            .iter()
            .map(|instruction| match instruction.canonical.as_str() {
                "fixture_a" => Ok(0xaa),
                "fixture_b" => Ok(0xbb),
                canonical => anyhow::bail!("unknown fixture instruction {canonical}"),
            })
            .collect::<Result<_>>()?;
        if self.reassembly_mismatch {
            output[1] ^= 0xff;
        }
        Ok(output)
    }
}

fn region(id: &str, range: Range<usize>, kind: RegionKind) -> ImageRegion {
    ImageRegion {
        id: id.to_owned(),
        range,
        kind,
        reason: "conformance fixture".to_owned(),
    }
}

fn write(
    id: &str,
    actor: &str,
    offset: usize,
    expected_original: &[u8],
    replacement: &[u8],
    intent: WriteIntent,
) -> ExpectedWrite {
    ExpectedWrite {
        id: id.to_owned(),
        actor: actor.to_owned(),
        purpose: "conformance fixture".to_owned(),
        offset,
        expected_original: expected_original.to_vec(),
        replacement: replacement.to_vec(),
        intent,
    }
}
