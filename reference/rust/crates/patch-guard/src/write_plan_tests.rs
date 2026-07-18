use anyhow::ensure;

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
    assert_eq!(plan.apply(&[0, 1, 2, 3], None).unwrap(), [0, 8, 9, 3]);
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
        plan.apply(&[0, 1], Some(&ExactAssemblyVerifier)).unwrap(),
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
    let mut output = complete.apply(&baseline, None).unwrap();
    output[0] = 9;
    assert!(complete.audit(&baseline, &output, None).is_err());
}
