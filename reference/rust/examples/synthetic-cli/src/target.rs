use std::ops::Range;

use anyhow::{Context, Result, bail, ensure};
use patch_guard::{
    ArtifactReport, BuildMode, BuildReport, ExpectedWrite, ImageRegion, ProductGraph, ProductStep,
    RegionKind, ResizePlan, RootArtifact, RootKind, SourceSpec, WriteIntent, WritePlan,
    evaluate_readiness, verify_source,
};

use crate::translation::{AssetDisposition, TranslationAsset, control_tokens};

pub const DEMO_SOURCE_ID: &str = "self-authored-synthetic-image-v1";
const DEMO_OUTPUT_ID: &str = "synthetic-patched-image-v1";
pub const DEMO_SOURCE_SHA256: &str =
    "e8a94f246ababf82ee6d9176a31b9415958f6adbdb3762ef861e58c7b81d68eb";
const DEMO_SOURCE_LEN: usize = 64;
const POINTER_RANGES: [Range<usize>; 2] = [4..6, 6..8];
const CHECKSUM_RANGE: Range<usize> = 8..10;
const IMMUTABLE_RANGE: Range<usize> = 10..64;
const ORIGINAL_TEXT_RANGES: [Range<usize>; 2] = [16..22, 22..28];
const ENTRY_IDS: [&str; 2] = ["demo.dialog.0001", "demo.dialog.0002"];
const SOURCE_TEXTS: [&str; 2] = ["HELLO{end}", "WORLD{end}"];

#[derive(Debug, Clone)]
pub struct BuildResult {
    pub output: Vec<u8>,
    pub report: BuildReport,
}

pub const fn source_spec() -> SourceSpec<'static> {
    SourceSpec {
        id: DEMO_SOURCE_ID,
        len: DEMO_SOURCE_LEN,
        sha256: DEMO_SOURCE_SHA256,
    }
}

pub fn demo_source() -> Vec<u8> {
    let mut source = Vec::with_capacity(DEMO_SOURCE_LEN);
    source.extend_from_slice(b"SYN1");
    source.extend_from_slice(&(ORIGINAL_TEXT_RANGES[0].start as u16).to_le_bytes());
    source.extend_from_slice(&(ORIGINAL_TEXT_RANGES[1].start as u16).to_le_bytes());
    source.extend_from_slice(&[0, 0]);
    source.extend_from_slice(b"GUARD!");
    source.extend_from_slice(b"HELLO\0");
    source.extend_from_slice(b"WORLD\0");
    source.resize(DEMO_SOURCE_LEN, 0xff);
    let checksum = checksum(&source).to_le_bytes();
    source[CHECKSUM_RANGE].copy_from_slice(&checksum);
    source
}

pub fn build(source: &[u8], translation_bytes: &[u8], mode: BuildMode) -> Result<BuildResult> {
    let verified = verify_source(source_spec(), source)?;
    ensure!(
        source.starts_with(b"SYN1"),
        "synthetic source magic mismatch"
    );
    ensure!(
        stored_checksum(source)? == checksum(source),
        "synthetic source checksum mismatch"
    );

    let asset = TranslationAsset::from_slice(translation_bytes)?;
    asset.validate_identity(&verified.id, &verified.sha256)?;
    validate_population(source, &asset)?;
    let readiness = evaluate_readiness(mode, &asset.localization_scope()?)?;
    let graph = product_graph(&verified.id, &asset.asset_id).validate()?;

    let mut payload = Vec::new();
    let mut pointer_updates = Vec::new();
    for (index, entry) in asset.entries.iter().enumerate() {
        if entry.build == AssetDisposition::UseLocalized {
            let pointer = source
                .len()
                .checked_add(payload.len())
                .context("synthetic text pointer overflow")?;
            ensure!(
                pointer <= u16::MAX as usize,
                "synthetic pointer exceeds u16"
            );
            let encoded = encode_demo_text(&entry.ko)?;
            pointer_updates.push((index, pointer as u16));
            payload.extend_from_slice(&encoded);
        }
    }
    ensure!(
        !payload.is_empty(),
        "synthetic build selected no localized text"
    );

    let output_len = source
        .len()
        .checked_add(payload.len())
        .context("synthetic output length overflow")?;
    let mut candidate = source.to_vec();
    candidate.resize(output_len, 0);
    candidate[source.len()..].copy_from_slice(&payload);
    for &(index, pointer) in &pointer_updates {
        candidate[POINTER_RANGES[index].clone()].copy_from_slice(&pointer.to_le_bytes());
    }
    let updated_checksum = checksum(&candidate).to_le_bytes();
    candidate[CHECKSUM_RANGE].copy_from_slice(&updated_checksum);

    let mut regions = vec![
        image_region("magic", 0..4, RegionKind::Protected),
        image_region("text-pointer-1", 4..6, RegionKind::Metadata),
        image_region("text-pointer-2", 6..8, RegionKind::Metadata),
        image_region("checksum", CHECKSUM_RANGE, RegionKind::Metadata),
        image_region("original-payload", IMMUTABLE_RANGE, RegionKind::Protected),
        image_region(
            "localized-payload",
            source.len()..output_len,
            RegionKind::Data,
        ),
    ];
    regions.sort_by_key(|region| region.range.start);

    let mut writes = vec![expected_write(
        "localized-payload",
        "text-layout",
        "append selected localized strings",
        source.len(),
        Vec::new(),
        payload.clone(),
        WriteIntent::Data,
    )];
    for &(index, _) in &pointer_updates {
        writes.push(expected_write(
            &format!("text-pointer-{}", index + 1),
            "text-layout",
            "redirect a selected text consumer",
            POINTER_RANGES[index].start,
            source[POINTER_RANGES[index].clone()].to_vec(),
            candidate[POINTER_RANGES[index].clone()].to_vec(),
            WriteIntent::Metadata,
        ));
    }
    writes.push(expected_write(
        "container-checksum",
        "container-integrity",
        "update the synthetic checksum after all planned writes",
        CHECKSUM_RANGE.start,
        source[CHECKSUM_RANGE].to_vec(),
        candidate[CHECKSUM_RANGE].to_vec(),
        WriteIntent::Metadata,
    ));

    let plan = WritePlan {
        resize: Some(ResizePlan {
            actor: "text-layout".to_owned(),
            purpose: "append selected localized strings".to_owned(),
            expected_input_len: source.len(),
            output_len,
        }),
        regions,
        writes,
    };
    let applied = plan.apply(source, None)?;
    ensure!(
        applied.output == candidate,
        "Expected Write plan differs from independently assembled output"
    );
    verify_output(source, &applied.output, &asset, &pointer_updates)?;

    let report = BuildReport {
        schema_version: 1,
        mode,
        release_candidate: readiness.release_candidate,
        source_inputs: vec![ArtifactReport::from_bytes(verified.id, source)?],
        authored_inputs: vec![ArtifactReport::from_bytes(
            asset.asset_id,
            translation_bytes,
        )?],
        output: ArtifactReport::from_bytes(DEMO_OUTPUT_ID, &applied.output)?,
        readiness,
        product_steps: graph.execution_order,
        resize: applied.resize,
        writes: applied.writes,
    };
    report.validate()?;
    Ok(BuildResult {
        output: applied.output,
        report,
    })
}

fn product_graph(source_id: &str, translation_id: &str) -> ProductGraph {
    ProductGraph {
        roots: vec![
            RootArtifact {
                id: source_id.to_owned(),
                kind: RootKind::PureSource,
            },
            RootArtifact {
                id: translation_id.to_owned(),
                kind: RootKind::PureSource,
            },
        ],
        steps: vec![
            ProductStep {
                id: "validate-translations".to_owned(),
                inputs: vec![source_id.to_owned(), translation_id.to_owned()],
                outputs: vec!["localization-plan".to_owned()],
            },
            ProductStep {
                id: "encode-localized-text".to_owned(),
                inputs: vec!["localization-plan".to_owned()],
                outputs: vec!["encoded-text".to_owned()],
            },
            ProductStep {
                id: "plan-image-writes".to_owned(),
                inputs: vec![source_id.to_owned(), "encoded-text".to_owned()],
                outputs: vec!["expected-write-plan".to_owned()],
            },
            ProductStep {
                id: "apply-and-audit".to_owned(),
                inputs: vec![source_id.to_owned(), "expected-write-plan".to_owned()],
                outputs: vec![DEMO_OUTPUT_ID.to_owned()],
            },
        ],
        final_artifacts: vec![DEMO_OUTPUT_ID.to_owned()],
    }
}

fn validate_population(source: &[u8], asset: &TranslationAsset) -> Result<()> {
    ensure!(
        asset.entries.len() == ENTRY_IDS.len(),
        "synthetic translation population mismatch"
    );
    for (index, entry) in asset.entries.iter().enumerate() {
        ensure!(
            entry.id == ENTRY_IDS[index],
            "unexpected synthetic translation entry {}",
            entry.id
        );
        ensure!(
            entry.raw_bytes()? == source[ORIGINAL_TEXT_RANGES[index].clone()],
            "protected source bytes differ for {}",
            entry.id
        );
        ensure!(
            entry.source_text == SOURCE_TEXTS[index],
            "protected source text differs for {}",
            entry.id
        );
        if entry.build == AssetDisposition::UseLocalized {
            ensure!(
                !entry.ko.is_empty(),
                "selected translation {} is empty",
                entry.id
            );
            ensure!(
                control_tokens(&entry.source_text)? == control_tokens(&entry.ko)?,
                "control tokens changed for {}",
                entry.id
            );
        }
    }
    Ok(())
}

fn verify_output(
    source: &[u8],
    output: &[u8],
    asset: &TranslationAsset,
    pointer_updates: &[(usize, u16)],
) -> Result<()> {
    ensure!(
        output[0..4] == source[0..4] && output[IMMUTABLE_RANGE] == source[IMMUTABLE_RANGE],
        "protected synthetic bytes changed"
    );
    ensure!(
        stored_checksum(output)? == checksum(output),
        "synthetic output checksum mismatch"
    );
    for (index, entry) in asset.entries.iter().enumerate() {
        let pointer =
            u16::from_le_bytes(output[POINTER_RANGES[index].clone()].try_into().unwrap()) as usize;
        if let Some((_, expected_pointer)) = pointer_updates
            .iter()
            .find(|(updated_index, _)| *updated_index == index)
        {
            let encoded = encode_demo_text(&entry.ko)?;
            ensure!(
                pointer == usize::from(*expected_pointer),
                "synthetic pointer mismatch for {}",
                entry.id
            );
            ensure!(
                output.get(pointer..pointer + encoded.len()) == Some(encoded.as_slice()),
                "synthetic encoded text mismatch for {}",
                entry.id
            );
        } else {
            ensure!(
                pointer == ORIGINAL_TEXT_RANGES[index].start,
                "source-preserved pointer changed for {}",
                entry.id
            );
        }
    }
    Ok(())
}

fn image_region(id: &str, range: Range<usize>, kind: RegionKind) -> ImageRegion {
    ImageRegion {
        id: id.to_owned(),
        range,
        kind,
        reason: "self-authored synthetic format boundary".to_owned(),
    }
}

#[allow(clippy::too_many_arguments)]
fn expected_write(
    id: &str,
    actor: &str,
    purpose: &str,
    offset: usize,
    expected_original: Vec<u8>,
    replacement: Vec<u8>,
    intent: WriteIntent,
) -> ExpectedWrite {
    ExpectedWrite {
        id: id.to_owned(),
        actor: actor.to_owned(),
        purpose: purpose.to_owned(),
        offset,
        expected_original,
        replacement,
        intent,
    }
}

fn stored_checksum(bytes: &[u8]) -> Result<u16> {
    let stored: [u8; 2] = bytes
        .get(CHECKSUM_RANGE)
        .context("synthetic checksum field is missing")?
        .try_into()
        .unwrap();
    Ok(u16::from_le_bytes(stored))
}

fn checksum(bytes: &[u8]) -> u16 {
    bytes
        .iter()
        .enumerate()
        .filter(|(index, _)| !CHECKSUM_RANGE.contains(index))
        .fold(0_u16, |sum, (_, byte)| sum.wrapping_add(u16::from(*byte)))
}

fn encode_demo_text(text: &str) -> Result<Vec<u8>> {
    let chars: Vec<char> = text.chars().collect();
    let mut encoded = Vec::new();
    let mut index = 0;
    while index < chars.len() {
        if chars[index] == '{' {
            let start = index;
            while index < chars.len() && chars[index] != '}' {
                index += 1;
            }
            ensure!(index < chars.len(), "unterminated control token");
            let token: String = chars[start..=index].iter().collect();
            ensure!(token == "{end}", "unsupported synthetic token {token}");
            encoded.push(0);
        } else {
            encoded.push(match chars[index] {
                '안' => 0x80,
                '녕' => 0x81,
                '세' => 0x82,
                '계' => 0x83,
                character => bail!("unsupported synthetic character {character}"),
            });
        }
        index += 1;
    }
    ensure!(
        encoded.last() == Some(&0),
        "synthetic text lacks an end token"
    );
    Ok(encoded)
}

#[cfg(test)]
mod tests {
    use super::*;
    use patch_guard::sha256_hex;

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
            u16::from_le_bytes(result.output[POINTER_RANGES[1].clone()].try_into().unwrap())
                as usize,
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
}
