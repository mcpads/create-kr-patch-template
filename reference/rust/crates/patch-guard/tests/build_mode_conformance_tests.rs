mod support;

use anyhow::Result;
use patch_guard::{
    BuildDisposition, BuildMode, LocalizationScope, LocalizationUnit, PopulationStatus,
    ReleaseApproval, ReviewState, evaluate_readiness, review_revision,
};

use support::run_manifest;

#[test]
fn build_mode_cases_match_language_neutral_expectations() {
    run_manifest("build-mode.json", run_build_mode_scenario);
}

fn run_build_mode_scenario(scenario: &str) -> Result<()> {
    let mut scope = incomplete_scope();
    let mode = match scenario {
        "development_preserves_incomplete" => BuildMode::Development,
        "development_uses_draft" => {
            scope.units[1].disposition = BuildDisposition::UseLocalized;
            BuildMode::Development
        }
        "development_population_in_progress" => {
            scope.population_status = PopulationStatus::InProgress;
            BuildMode::Development
        }
        "development_known_unit_omitted" => {
            scope.units.pop();
            BuildMode::Development
        }
        "release_incomplete" => {
            approve(&mut scope)?;
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
        "release_population_unconfirmed" => {
            complete_scope(&mut scope);
            scope.population_status = PopulationStatus::InProgress;
            approve(&mut scope)?;
            BuildMode::ReleaseCandidate
        }
        "release_population_changed_after_approval" => {
            complete_scope(&mut scope);
            approve(&mut scope)?;
            scope.known_unit_ids.push("line.3".to_owned());
            scope.units.push(LocalizationUnit {
                id: "line.3".to_owned(),
                disposition: BuildDisposition::UseLocalized,
                review_state: ReviewState::Complete,
            });
            BuildMode::ReleaseCandidate
        }
        "release_complete_approved" => {
            complete_scope(&mut scope);
            approve(&mut scope)?;
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
        population_status: PopulationStatus::Confirmed,
        known_unit_ids: vec!["line.1".to_owned(), "line.2".to_owned()],
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

fn approve(scope: &mut LocalizationScope) -> Result<()> {
    scope.release_approval = ReleaseApproval::Approved;
    scope.approved_revision = Some(review_revision(scope)?);
    Ok(())
}
