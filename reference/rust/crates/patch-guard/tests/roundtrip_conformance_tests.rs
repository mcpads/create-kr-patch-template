mod support;

use anyhow::Result;
use patch_guard::verify_exact_roundtrip;

use support::run_manifest;

#[test]
fn roundtrip_cases_match_language_neutral_expectations() {
    run_manifest("roundtrip.json", run_roundtrip_scenario);
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
