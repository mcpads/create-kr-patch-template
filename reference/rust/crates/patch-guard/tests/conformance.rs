use std::{fs, ops::Range, path::PathBuf};

use anyhow::{Result, ensure};
use patch_guard::{
    ArtifactReport, BuildDisposition, BuildMode, ExpectedWrite, ImageRegion, LocalizationScope,
    LocalizationUnit, MachineCodeCheck, MachineCodeProvenance, MachineCodeVerifier, ProductGraph,
    ProductStep, RegionKind, ReleaseApproval, ReviewState, RootArtifact, RootKind,
    RuntimeEvidenceReport, RuntimeOutcome, WriteIntent, WritePlan, evaluate_readiness,
    require_runtime_pass, verify_exact_roundtrip,
};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct Manifest {
    schema_version: u32,
    cases: Vec<Case>,
}

#[derive(Debug, Deserialize)]
struct Case {
    id: String,
    scenario: String,
    expected: Expected,
    given: String,
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum Expected {
    Accept,
    Reject,
}

#[test]
fn write_plan_cases_match_language_neutral_expectations() {
    run_manifest("write-plan.json", run_write_scenario);
}

#[test]
fn product_graph_cases_match_language_neutral_expectations() {
    run_manifest("product-graph.json", run_graph_scenario);
}

#[test]
fn build_mode_cases_match_language_neutral_expectations() {
    run_manifest("build-mode.json", run_build_mode_scenario);
}

#[test]
fn roundtrip_cases_match_language_neutral_expectations() {
    run_manifest("roundtrip.json", run_roundtrip_scenario);
}

#[test]
fn runtime_evidence_cases_match_language_neutral_expectations() {
    run_manifest("runtime-evidence.json", run_runtime_evidence_scenario);
}

fn run_manifest(path: &str, runner: fn(&str) -> Result<()>) {
    let manifest = load_manifest(path);
    assert_eq!(manifest.schema_version, 1, "unsupported manifest {path}");
    assert!(!manifest.cases.is_empty(), "empty manifest {path}");
    for case in manifest.cases {
        let actual = if runner(&case.scenario).is_ok() {
            Expected::Accept
        } else {
            Expected::Reject
        };
        assert_eq!(
            actual, case.expected,
            "conformance case {} failed: {}",
            case.id, case.given
        );
    }
}

fn load_manifest(path: &str) -> Manifest {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../../../conformance")
        .join(path);
    let bytes = fs::read(&path).unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
    serde_json::from_slice(&bytes)
        .unwrap_or_else(|error| panic!("parse {}: {error}", path.display()))
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
            let mut output = plan.apply(&baseline, None)?.output;
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
            .apply(&baseline, Some(&FixtureIsaVerifier))
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

struct FixtureIsaVerifier;

impl MachineCodeVerifier for FixtureIsaVerifier {
    fn assemble(&self, check: &MachineCodeCheck<'_>) -> Result<Vec<u8>> {
        ensure!(check.provenance.assembly_source_id == "asm/hook.s");
        ensure!(check.provenance.isa_profile_id == "fixture-isa-v1");
        Ok(vec![0xaa, 0xbb])
    }

    fn decoded_len(&self, check: &MachineCodeCheck<'_>) -> Result<usize> {
        ensure!(check.write.replacement == [0xaa, 0xbb]);
        Ok(2)
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

fn run_graph_scenario(scenario: &str) -> Result<()> {
    let mut graph = pure_graph();
    match scenario {
        "pure_source_and_product_derivative" => {}
        "research_output_as_product_input" => graph.roots[0].kind = RootKind::ResearchOutput,
        "external_derived_product_input" => graph.roots[0].kind = RootKind::ExternalDerived,
        "missing_producer" => graph.steps[1].inputs.push("missing".to_owned()),
        "duplicate_producer" => graph.steps[1].outputs = vec!["decoded".to_owned()],
        "dependency_cycle" => graph.steps[0].inputs = vec!["patched".to_owned()],
        "dead_product_step" => graph.steps.push(ProductStep {
            id: "research".to_owned(),
            inputs: vec!["source".to_owned()],
            outputs: vec!["notes".to_owned()],
        }),
        other => panic!("unknown product-graph conformance scenario {other}"),
    }
    graph.validate().map(|_| ())
}

fn pure_graph() -> ProductGraph {
    ProductGraph {
        roots: vec![RootArtifact {
            id: "source".to_owned(),
            kind: RootKind::PureSource,
        }],
        steps: vec![
            ProductStep {
                id: "decode".to_owned(),
                inputs: vec!["source".to_owned()],
                outputs: vec!["decoded".to_owned()],
            },
            ProductStep {
                id: "build".to_owned(),
                inputs: vec!["source".to_owned(), "decoded".to_owned()],
                outputs: vec!["patched".to_owned()],
            },
        ],
        final_artifacts: vec!["patched".to_owned()],
    }
}

fn run_build_mode_scenario(scenario: &str) -> Result<()> {
    let mut scope = incomplete_scope();
    let mode = match scenario {
        "development_preserves_incomplete" => BuildMode::Development,
        "development_uses_draft" => {
            scope.units[1].disposition = BuildDisposition::UseLocalized;
            BuildMode::Development
        }
        "release_incomplete" => {
            scope.release_approval = ReleaseApproval::Approved;
            BuildMode::ReleaseCandidate
        }
        "release_unapproved" => {
            complete_scope(&mut scope);
            BuildMode::ReleaseCandidate
        }
        "release_changed_after_approval" => {
            complete_scope(&mut scope);
            scope.release_approval = ReleaseApproval::Approved;
            scope.approved_revision = Some("older-revision".to_owned());
            BuildMode::ReleaseCandidate
        }
        "release_complete_approved" => {
            complete_scope(&mut scope);
            scope.release_approval = ReleaseApproval::Approved;
            scope.approved_revision = Some(scope.content_revision.clone());
            BuildMode::ReleaseCandidate
        }
        other => panic!("unknown build-mode conformance scenario {other}"),
    };
    evaluate_readiness(mode, &scope).map(|_| ())
}

fn incomplete_scope() -> LocalizationScope {
    LocalizationScope {
        id: "declared-scope".to_owned(),
        content_revision: "current-revision".to_owned(),
        release_approval: ReleaseApproval::Pending,
        approved_revision: None,
        units: vec![
            LocalizationUnit {
                id: "line.1".to_owned(),
                disposition: BuildDisposition::UseLocalized,
                review_state: ReviewState::Complete,
            },
            LocalizationUnit {
                id: "line.2".to_owned(),
                disposition: BuildDisposition::PreserveSource,
                review_state: ReviewState::Draft,
            },
        ],
    }
}

fn complete_scope(scope: &mut LocalizationScope) {
    for unit in &mut scope.units {
        unit.disposition = BuildDisposition::UseLocalized;
        unit.review_state = ReviewState::Complete;
    }
}

fn run_roundtrip_scenario(scenario: &str) -> Result<()> {
    let original = b"header-payload-tail";
    let rebuilt = match scenario {
        "complete_boundary_is_identical" => original.to_vec(),
        "tail_diff_hidden_by_payload_check" => {
            let mut bytes = original.to_vec();
            *bytes.last_mut().unwrap() ^= 1;
            bytes
        }
        "partial_boundary_comparison" => original[..14].to_vec(),
        other => panic!("unknown round-trip conformance scenario {other}"),
    };
    verify_exact_roundtrip("container", original, &rebuilt).map(|_| ())
}

fn run_runtime_evidence_scenario(scenario: &str) -> Result<()> {
    let expected = artifact("patched-image", 'a');
    let mut report = RuntimeEvidenceReport {
        schema_version: 1,
        scenario_id: "boot-and-open-dialog".to_owned(),
        artifact: expected.clone(),
        outcome: RuntimeOutcome::Passed,
        evidence: vec![artifact("trace", 'b')],
    };
    match scenario {
        "passed_evidence_for_exact_artifact" => {}
        "evidence_from_different_build" => report.artifact = artifact("patched-image", 'c'),
        "passed_without_evidence" => report.evidence.clear(),
        "failed_runtime_scenario" => report.outcome = RuntimeOutcome::Failed,
        other => panic!("unknown runtime-evidence conformance scenario {other}"),
    }
    require_runtime_pass(&expected, &report)
}

fn artifact(id: &str, hex_digit: char) -> ArtifactReport {
    ArtifactReport::from_bytes(id, &[hex_digit as u8]).unwrap()
}
