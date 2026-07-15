use serde_json::{Value, json};

use crate::tools::{Dispatch, dispatch};

fn decision(name: &str, args: Value) -> bool {
    match dispatch(name, &args) {
        Dispatch::Judged(judgment) => judgment.accept,
        Dispatch::InvalidParams(message) => panic!("{name} invalid params: {message}"),
        Dispatch::UnknownTool => panic!("{name} is not a registered tool"),
    }
}

fn report(name: &str, args: Value) -> Value {
    match dispatch(name, &args) {
        Dispatch::Judged(judgment) => judgment.report,
        _ => panic!("expected a judgment from {name}, got another dispatch outcome"),
    }
}

// Bytes [0,1,2,3] hash, shared by the source and roundtrip cases.
const BASELINE_SHA256: &str = "054edec1d0211f624fed0cbca9d4f9400b0e491c43742af2c5b0abebf0c990d8";

#[test]
fn verify_source_accepts_matching_identity_and_rejects_mismatch() {
    assert!(decision(
        "verify_source",
        json!({
            "id": "rom",
            "expected_len": 4,
            "expected_sha256": BASELINE_SHA256,
            "bytes": [0, 1, 2, 3]
        })
    ));
    assert!(!decision(
        "verify_source",
        json!({
            "id": "rom",
            "expected_len": 4,
            "expected_sha256": BASELINE_SHA256,
            "bytes": [0, 1, 2, 9]
        })
    ));
}

#[test]
fn roundtrip_rejects_any_boundary_byte_difference() {
    assert!(decision(
        "verify_exact_roundtrip",
        json!({ "boundary_id": "header", "original": [1, 2, 3], "rebuilt": [1, 2, 3] })
    ));
    // tail_diff_hidden_by_payload_check
    assert!(!decision(
        "verify_exact_roundtrip",
        json!({ "boundary_id": "header", "original": [1, 2, 3], "rebuilt": [1, 2, 9] })
    ));
    // partial_boundary_comparison
    assert!(!decision(
        "verify_exact_roundtrip",
        json!({ "boundary_id": "header", "original": [1, 2, 3], "rebuilt": [1, 2] })
    ));
}

fn scope(units: Value, approval: &str, approved_revision: Value) -> Value {
    json!({
        "id": "scope-1",
        "content_revision": "rev-1",
        "release_approval": approval,
        "approved_revision": approved_revision,
        "units": units
    })
}

#[test]
fn readiness_matches_build_mode_conformance() {
    let complete =
        json!([{ "id": "u1", "disposition": "use_localized", "review_state": "complete" }]);

    // development_preserves_incomplete
    assert!(decision(
        "evaluate_readiness",
        json!({
            "mode": "development",
            "scope": scope(
                json!([
                    { "id": "u1", "disposition": "use_localized", "review_state": "complete" },
                    { "id": "u2", "disposition": "preserve_source", "review_state": "untranslated" }
                ]),
                "pending",
                Value::Null
            )
        })
    ));

    // release_incomplete
    assert!(!decision(
        "evaluate_readiness",
        json!({
            "mode": "release_candidate",
            "scope": scope(
                json!([
                    { "id": "u1", "disposition": "use_localized", "review_state": "complete" },
                    { "id": "u2", "disposition": "preserve_source", "review_state": "complete" }
                ]),
                "approved",
                json!("rev-1")
            )
        })
    ));

    // release_unapproved
    assert!(!decision(
        "evaluate_readiness",
        json!({ "mode": "release_candidate", "scope": scope(complete.clone(), "pending", Value::Null) })
    ));

    // release_changed_after_approval
    assert!(!decision(
        "evaluate_readiness",
        json!({ "mode": "release_candidate", "scope": scope(complete.clone(), "approved", json!("rev-0")) })
    ));

    // release_complete_approved
    assert!(decision(
        "evaluate_readiness",
        json!({ "mode": "release_candidate", "scope": scope(complete, "approved", json!("rev-1")) })
    ));
}

#[test]
fn product_graph_matches_conformance() {
    // pure_source_and_product_derivative
    assert!(decision(
        "validate_product_graph",
        json!({
            "roots": [{ "id": "src", "kind": "pure_source" }],
            "steps": [{ "id": "s1", "inputs": ["src"], "outputs": ["image"] }],
            "final_artifacts": ["image"]
        })
    ));

    // research_output_as_product_input
    assert!(!decision(
        "validate_product_graph",
        json!({
            "roots": [{ "id": "poc", "kind": "research_output" }],
            "steps": [{ "id": "s1", "inputs": ["poc"], "outputs": ["image"] }],
            "final_artifacts": ["image"]
        })
    ));

    // duplicate_producer
    assert!(!decision(
        "validate_product_graph",
        json!({
            "roots": [{ "id": "src", "kind": "pure_source" }],
            "steps": [
                { "id": "s1", "inputs": ["src"], "outputs": ["image"] },
                { "id": "s2", "inputs": ["src"], "outputs": ["image"] }
            ],
            "final_artifacts": ["image"]
        })
    ));

    // dependency_cycle
    assert!(!decision(
        "validate_product_graph",
        json!({
            "roots": [{ "id": "src", "kind": "pure_source" }],
            "steps": [
                { "id": "s1", "inputs": ["src", "b"], "outputs": ["a"] },
                { "id": "s2", "inputs": ["a"], "outputs": ["b"] }
            ],
            "final_artifacts": ["a"]
        })
    ));
}

fn artifact(id: &str, len: usize, sha: &str) -> Value {
    json!({ "id": id, "len": len, "sha256": sha })
}

#[test]
fn runtime_evidence_matches_conformance() {
    let sha = "a".repeat(64);
    let build = artifact("out.bin", 16, &sha);

    // passed_evidence_for_exact_artifact
    assert!(decision(
        "require_runtime_pass",
        json!({
            "expected_artifact": build,
            "report": {
                "schema_version": 1,
                "scenario_id": "boot",
                "artifact": artifact("out.bin", 16, &sha),
                "outcome": "passed",
                "evidence": [artifact("frame.png", 8, &"b".repeat(64))]
            }
        })
    ));

    // evidence_from_different_build
    assert!(!decision(
        "require_runtime_pass",
        json!({
            "expected_artifact": artifact("out.bin", 16, &sha),
            "report": {
                "schema_version": 1,
                "scenario_id": "boot",
                "artifact": artifact("out.bin", 16, &"c".repeat(64)),
                "outcome": "passed",
                "evidence": [artifact("frame.png", 8, &"b".repeat(64))]
            }
        })
    ));

    // passed_without_evidence
    assert!(!decision(
        "require_runtime_pass",
        json!({
            "expected_artifact": artifact("out.bin", 16, &sha),
            "report": {
                "schema_version": 1,
                "scenario_id": "boot",
                "artifact": artifact("out.bin", 16, &sha),
                "outcome": "passed",
                "evidence": []
            }
        })
    ));

    // failed_runtime_scenario
    assert!(!decision(
        "require_runtime_pass",
        json!({
            "expected_artifact": artifact("out.bin", 16, &sha),
            "report": {
                "schema_version": 1,
                "scenario_id": "boot",
                "artifact": artifact("out.bin", 16, &sha),
                "outcome": "failed",
                "evidence": [artifact("frame.png", 8, &"b".repeat(64))]
            }
        })
    ));
}

fn data_region() -> Value {
    json!({ "id": "data", "start": 1, "end": 3, "kind": "data", "reason": "text pool" })
}

fn data_write() -> Value {
    json!({
        "id": "first",
        "actor": "layout",
        "purpose": "place",
        "offset": 1,
        "expected_original": [1, 2],
        "replacement": [8, 9],
        "intent": "data"
    })
}

#[test]
fn write_plan_matches_conformance() {
    // owned_data_write
    let owned = report(
        "apply_write_plan",
        json!({ "baseline": [0, 1, 2, 3], "regions": [data_region()], "writes": [data_write()] }),
    );
    assert_eq!(owned["output"], json!([0, 8, 9, 3]));
    assert_eq!(owned["output_sha256"].as_str().map(str::len), Some(64));

    // overlapping_writes
    let mut second = data_write();
    second["id"] = json!("second");
    second["offset"] = json!(2);
    second["expected_original"] = json!([2]);
    second["replacement"] = json!([7]);
    assert!(!decision(
        "apply_write_plan",
        json!({
            "baseline": [0, 1, 2, 3],
            "regions": [{ "id": "data", "start": 0, "end": 4, "kind": "data", "reason": "text pool" }],
            "writes": [data_write(), second]
        })
    ));

    // protected_region_write
    assert!(!decision(
        "apply_write_plan",
        json!({
            "baseline": [0, 1, 2, 3],
            "regions": [{ "id": "data", "start": 1, "end": 3, "kind": "protected", "reason": "checksum" }],
            "writes": [data_write()]
        })
    ));

    // wrong_original_bytes
    let mut wrong = data_write();
    wrong["expected_original"] = json!([9, 9]);
    assert!(!decision(
        "apply_write_plan",
        json!({ "baseline": [0, 1, 2, 3], "regions": [data_region()], "writes": [wrong] })
    ));

    // machine_code_without_verifier
    assert!(!decision(
        "apply_write_plan",
        json!({
            "baseline": [0, 1, 2, 3],
            "regions": [{ "id": "code", "start": 1, "end": 3, "kind": "machine_code", "reason": "hook" }],
            "writes": [{
                "id": "hook",
                "actor": "layout",
                "purpose": "code-patch",
                "offset": 1,
                "expected_original": [1, 2],
                "replacement": [170, 187],
                "intent": "machine_code",
                "machine_code": { "assembly_source_id": "asm/hook.s", "isa_profile_id": "isa-v1" }
            }]
        })
    ));
}

#[test]
fn malformed_arguments_report_invalid_params() {
    match dispatch("verify_source", &json!({ "id": "rom" })) {
        Dispatch::InvalidParams(_) => {}
        _ => panic!("missing fields must report invalid params, not a judgment"),
    }
    match dispatch("does_not_exist", &json!({})) {
        Dispatch::UnknownTool => {}
        _ => panic!("unknown tool must be reported"),
    }
}
