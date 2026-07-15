use std::collections::{BTreeMap, BTreeSet};

use anyhow::{Result, bail, ensure};
use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RootKind {
    PureSource,
    ExternalDerived,
    ResearchOutput,
}

#[derive(Debug, Clone)]
pub struct RootArtifact {
    pub id: String,
    pub kind: RootKind,
}

#[derive(Debug, Clone)]
pub struct ProductStep {
    pub id: String,
    pub inputs: Vec<String>,
    pub outputs: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct ProductGraph {
    pub roots: Vec<RootArtifact>,
    pub steps: Vec<ProductStep>,
    pub final_artifacts: Vec<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ProductGraphReport {
    pub execution_order: Vec<String>,
    pub final_artifacts: Vec<String>,
}

impl ProductGraph {
    pub fn validate(&self) -> Result<ProductGraphReport> {
        ensure!(!self.steps.is_empty(), "product graph has no steps");
        ensure!(
            !self.final_artifacts.is_empty(),
            "product graph has no final artifacts"
        );

        let mut roots = BTreeMap::new();
        for root in &self.roots {
            ensure_nonempty("root artifact id", &root.id)?;
            ensure!(
                roots.insert(root.id.as_str(), root.kind).is_none(),
                "duplicate root artifact id {}",
                root.id
            );
        }

        let mut step_ids = BTreeSet::new();
        let mut producers = BTreeMap::new();
        for (index, step) in self.steps.iter().enumerate() {
            ensure_nonempty("product step id", &step.id)?;
            ensure!(
                step_ids.insert(step.id.as_str()),
                "duplicate product step id {}",
                step.id
            );
            ensure!(
                !step.inputs.is_empty(),
                "product step {} has no inputs",
                step.id
            );
            ensure!(
                !step.outputs.is_empty(),
                "product step {} has no outputs",
                step.id
            );
            ensure_unique_values("input", &step.id, &step.inputs)?;
            ensure_unique_values("output", &step.id, &step.outputs)?;

            for output in &step.outputs {
                ensure_nonempty("product artifact id", output)?;
                ensure!(
                    !roots.contains_key(output.as_str()),
                    "product output {output} shadows a root artifact"
                );
                ensure!(
                    producers.insert(output.as_str(), index).is_none(),
                    "product artifact {output} has more than one producer"
                );
            }
        }

        let mut dependencies = vec![BTreeSet::new(); self.steps.len()];
        let mut dependents = vec![BTreeSet::new(); self.steps.len()];
        for (index, step) in self.steps.iter().enumerate() {
            for input in &step.inputs {
                ensure_nonempty("product input id", input)?;
                if let Some(kind) = roots.get(input.as_str()) {
                    match kind {
                        RootKind::PureSource => {}
                        RootKind::ExternalDerived => {
                            bail!(
                                "product step {} consumes external derived artifact {input}",
                                step.id
                            );
                        }
                        RootKind::ResearchOutput => {
                            bail!(
                                "product step {} consumes research or PoC output {input}",
                                step.id
                            );
                        }
                    }
                } else if let Some(&producer) = producers.get(input.as_str()) {
                    dependencies[index].insert(producer);
                    dependents[producer].insert(index);
                } else {
                    bail!(
                        "product step {} consumes artifact {input} with no source or producer",
                        step.id
                    );
                }
            }
        }

        let mut final_producers = BTreeSet::new();
        let mut final_ids = BTreeSet::new();
        for artifact in &self.final_artifacts {
            ensure_nonempty("final artifact id", artifact)?;
            ensure!(
                final_ids.insert(artifact.as_str()),
                "duplicate final artifact id {artifact}"
            );
            let Some(&producer) = producers.get(artifact.as_str()) else {
                bail!("final artifact {artifact} is not produced by the product graph");
            };
            final_producers.insert(producer);
        }

        let reachable = dependencies_reaching_finals(&dependencies, &final_producers);
        for (index, step) in self.steps.iter().enumerate() {
            ensure!(
                reachable.contains(&index),
                "product step {} does not contribute to a final artifact",
                step.id
            );
        }

        let mut indegree: Vec<usize> = dependencies.iter().map(BTreeSet::len).collect();
        let mut ready = BTreeSet::new();
        for (index, degree) in indegree.iter().enumerate() {
            if *degree == 0 {
                ready.insert((self.steps[index].id.clone(), index));
            }
        }
        let mut order = Vec::with_capacity(self.steps.len());
        while let Some((_, index)) = ready.pop_first() {
            order.push(self.steps[index].id.clone());
            for &dependent in &dependents[index] {
                indegree[dependent] -= 1;
                if indegree[dependent] == 0 {
                    ready.insert((self.steps[dependent].id.clone(), dependent));
                }
            }
        }
        ensure!(
            order.len() == self.steps.len(),
            "product graph contains a dependency cycle"
        );

        Ok(ProductGraphReport {
            execution_order: order,
            final_artifacts: self.final_artifacts.clone(),
        })
    }
}

fn dependencies_reaching_finals(
    dependencies: &[BTreeSet<usize>],
    finals: &BTreeSet<usize>,
) -> BTreeSet<usize> {
    let mut reachable = finals.clone();
    let mut pending: Vec<usize> = finals.iter().copied().collect();
    while let Some(step) = pending.pop() {
        for &dependency in &dependencies[step] {
            if reachable.insert(dependency) {
                pending.push(dependency);
            }
        }
    }
    reachable
}

fn ensure_nonempty(label: &str, value: &str) -> Result<()> {
    ensure!(!value.trim().is_empty(), "{label} is empty");
    Ok(())
}

fn ensure_unique_values(label: &str, step: &str, values: &[String]) -> Result<()> {
    let mut seen = BTreeSet::new();
    for value in values {
        ensure!(
            seen.insert(value.as_str()),
            "product step {step} repeats {label} {value}"
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
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
}
