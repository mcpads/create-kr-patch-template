use std::{fs, path::PathBuf};

use anyhow::Result;
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

pub fn run_manifest(path: &str, runner: fn(&str) -> Result<()>) {
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
