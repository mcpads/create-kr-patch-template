use std::ops::Range;

use anyhow::{Context, Result, ensure};
use patch_guard::{
    ArtifactReport, BuildMode, BuildReport, ExpectedWrite, ImageRegion, ProductGraph, ProductStep,
    RegionKind, ResizePlan, RootArtifact, RootKind, WriteIntent, WritePlan, evaluate_readiness,
    verify_source,
};

use crate::translation::{AssetDisposition, TranslationAsset};

use super::format::{
    CHECKSUM_RANGE, DEMO_OUTPUT_ID, IMMUTABLE_RANGE, POINTER_RANGES, encode_demo_text, source_spec,
    validate_population, verify_output,
};

#[derive(Debug, Clone)]
pub struct BuildResult {
    pub output: Vec<u8>,
    pub report: BuildReport,
}

pub fn build(source: &[u8], translation_bytes: &[u8], mode: BuildMode) -> Result<BuildResult> {
    let verified = verify_source(source_spec(), source)?;
    ensure!(
        source.starts_with(b"SYN1"),
        "synthetic source magic mismatch"
    );
    validate_population_source(source)?;

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
    let updated_checksum = super::format::checksum(&candidate).to_le_bytes();
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

fn validate_population_source(source: &[u8]) -> Result<()> {
    ensure!(
        super::format::stored_checksum(source)? == super::format::checksum(source),
        "synthetic source checksum mismatch"
    );
    Ok(())
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
