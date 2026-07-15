use super::*;

fn scope() -> LocalizationScope {
    LocalizationScope {
        id: "demo".to_owned(),
        content_revision: "revision-current".to_owned(),
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
    assert_eq!(report.localized_units, 1);
    assert_eq!(report.source_preserved_units, 1);
    assert_eq!(report.unresolved_units, ["line.2"]);
}

#[test]
fn release_candidate_requires_completion_and_approval() {
    assert!(evaluate_readiness(BuildMode::ReleaseCandidate, &scope()).is_err());

    let mut complete = scope();
    complete.release_approval = ReleaseApproval::Approved;
    complete.approved_revision = Some(complete.content_revision.clone());
    complete.units[1].disposition = BuildDisposition::UseLocalized;
    complete.units[1].review_state = ReviewState::Complete;
    assert!(
        evaluate_readiness(BuildMode::ReleaseCandidate, &complete)
            .unwrap()
            .release_candidate
    );
}
