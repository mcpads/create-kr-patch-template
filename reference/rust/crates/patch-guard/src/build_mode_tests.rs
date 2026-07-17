use super::*;

fn scope() -> LocalizationScope {
    LocalizationScope {
        id: "demo".to_owned(),
        content_revision: "revision-current".to_owned(),
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

#[test]
fn development_build_reports_but_does_not_block_unfinished_units() {
    let report = evaluate_readiness(BuildMode::Development, &scope()).unwrap();
    assert!(!report.release_candidate);
    assert_eq!(report.population_status, PopulationStatus::Confirmed);
    assert_eq!(report.known_population_units, 2);
    assert_eq!(report.localized_units, 1);
    assert_eq!(report.source_preserved_units, 1);
    assert_eq!(report.unresolved_units, ["line.2"]);
}

#[test]
fn development_allows_population_in_progress_but_not_omitted_known_units() {
    let mut in_progress = scope();
    in_progress.population_status = PopulationStatus::InProgress;
    assert!(evaluate_readiness(BuildMode::Development, &in_progress).is_ok());

    in_progress.units.pop();
    assert!(evaluate_readiness(BuildMode::Development, &in_progress).is_err());
}

#[test]
fn release_candidate_requires_completion_and_approval() {
    assert!(evaluate_readiness(BuildMode::ReleaseCandidate, &scope()).is_err());

    let mut complete = scope();
    complete.units[1].disposition = BuildDisposition::UseLocalized;
    complete.units[1].review_state = ReviewState::Complete;
    complete.release_approval = ReleaseApproval::Approved;
    complete.approved_revision = Some(review_revision(&complete).unwrap());
    assert!(
        evaluate_readiness(BuildMode::ReleaseCandidate, &complete)
            .unwrap()
            .release_candidate
    );
}

#[test]
fn population_status_change_invalidates_release_approval() {
    let mut complete = scope();
    for unit in &mut complete.units {
        unit.disposition = BuildDisposition::UseLocalized;
        unit.review_state = ReviewState::Complete;
    }
    complete.population_status = PopulationStatus::InProgress;
    complete.release_approval = ReleaseApproval::Approved;
    complete.approved_revision = Some(review_revision(&complete).unwrap());
    complete.population_status = PopulationStatus::Confirmed;

    assert!(evaluate_readiness(BuildMode::ReleaseCandidate, &complete).is_err());
}
