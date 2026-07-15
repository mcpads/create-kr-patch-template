use patch_guard::{BuildMode, sha256_hex};

use super::format::{
    DEMO_SOURCE_LEN, DEMO_SOURCE_SHA256, ENTRY_IDS, ORIGINAL_TEXT_RANGES, POINTER_RANGES, checksum,
    stored_checksum,
};
use super::*;

const IN_PROGRESS: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/assets/demo.in-progress.json"
));
const COMPLETE: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/assets/demo.complete.json"
));

#[test]
fn generated_source_matches_explicit_identity() {
    let source = demo_source();
    assert_eq!(source.len(), DEMO_SOURCE_LEN);
    assert_eq!(sha256_hex(&source), DEMO_SOURCE_SHA256);
    assert_eq!(stored_checksum(&source).unwrap(), checksum(&source));
}

#[test]
fn development_build_preserves_unfinished_source_unit() {
    let result = build(&demo_source(), IN_PROGRESS, BuildMode::Development).unwrap();
    assert!(!result.report.release_candidate);
    assert_eq!(result.report.readiness.unresolved_units, [ENTRY_IDS[1]]);
    assert_eq!(result.report.readiness.source_preserved_units, 1);
    assert_eq!(
        u16::from_le_bytes(result.output[POINTER_RANGES[1].clone()].try_into().unwrap()) as usize,
        ORIGINAL_TEXT_RANGES[1].start
    );
}

#[test]
fn release_candidate_requires_complete_asset() {
    assert!(build(&demo_source(), IN_PROGRESS, BuildMode::ReleaseCandidate).is_err());
    let result = build(&demo_source(), COMPLETE, BuildMode::ReleaseCandidate).unwrap();
    assert!(result.report.release_candidate);
    assert!(result.report.readiness.unresolved_units.is_empty());
}

#[test]
fn source_protected_fields_and_reviewed_translation_cannot_drift() {
    let mut source = demo_source();
    source[20] ^= 1;
    assert!(build(&source, COMPLETE, BuildMode::ReleaseCandidate).is_err());

    let changed = String::from_utf8(COMPLETE.to_vec())
        .unwrap()
        .replace("48 45 4C 4C 4F 00", "48 45 4C 4C 4E 00");
    assert!(
        build(
            &demo_source(),
            changed.as_bytes(),
            BuildMode::ReleaseCandidate
        )
        .is_err()
    );

    let changed_after_approval = String::from_utf8(COMPLETE.to_vec())
        .unwrap()
        .replace("안녕{end}", "세계{end}");
    assert!(
        build(
            &demo_source(),
            changed_after_approval.as_bytes(),
            BuildMode::ReleaseCandidate
        )
        .is_err()
    );
}
