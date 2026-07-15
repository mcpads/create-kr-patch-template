mod support;

use anyhow::Result;
use patch_guard::{ProductGraph, ProductStep, RootArtifact, RootKind};

use support::run_manifest;

#[test]
fn product_graph_cases_match_language_neutral_expectations() {
    run_manifest("product-graph.json", run_graph_scenario);
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
