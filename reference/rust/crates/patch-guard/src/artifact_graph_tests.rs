use super::*;

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

#[test]
fn accepts_pure_sources_and_in_graph_derivatives() {
    let report = pure_graph().validate().unwrap();
    assert_eq!(report.execution_order, ["decode", "build"]);
}

#[test]
fn rejects_external_or_research_derivatives() {
    for kind in [RootKind::ExternalDerived, RootKind::ResearchOutput] {
        let mut graph = pure_graph();
        graph.roots[0].kind = kind;
        assert!(graph.validate().is_err());
    }
}

#[test]
fn rejects_duplicate_producers_cycles_and_dead_steps() {
    let mut graph = pure_graph();
    graph.steps[1].outputs = vec!["decoded".to_owned()];
    assert!(graph.validate().is_err());

    let mut graph = pure_graph();
    graph.steps[0].inputs = vec!["patched".to_owned()];
    assert!(graph.validate().is_err());

    let mut graph = pure_graph();
    graph.steps.push(ProductStep {
        id: "research".to_owned(),
        inputs: vec!["source".to_owned()],
        outputs: vec!["notes".to_owned()],
    });
    assert!(graph.validate().is_err());
}
