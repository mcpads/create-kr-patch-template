use std::collections::BTreeSet;

use anyhow::{Result, ensure};
use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BuildMode {
    Development,
    ReleaseCandidate,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BuildDisposition {
    PreserveSource,
    UseLocalized,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReviewState {
    Untranslated,
    Draft,
    NeedsReview,
    NeedsHumanReview,
    Complete,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReleaseApproval {
    Pending,
    Approved,
    Rejected,
}

#[derive(Debug, Clone)]
pub struct LocalizationUnit {
    pub id: String,
    pub disposition: BuildDisposition,
    pub review_state: ReviewState,
}

#[derive(Debug, Clone)]
pub struct LocalizationScope {
    pub id: String,
    pub content_revision: String,
    pub release_approval: ReleaseApproval,
    pub approved_revision: Option<String>,
    pub units: Vec<LocalizationUnit>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ReadinessReport {
    pub scope_id: String,
    pub mode: BuildMode,
    pub release_candidate: bool,
    pub localized_units: usize,
    pub source_preserved_units: usize,
    pub unresolved_units: Vec<String>,
}

pub fn evaluate_readiness(mode: BuildMode, scope: &LocalizationScope) -> Result<ReadinessReport> {
    ensure!(
        !scope.id.trim().is_empty(),
        "localization scope id is empty"
    );
    ensure!(
        !scope.units.is_empty(),
        "localization scope {} has no units",
        scope.id
    );
    ensure!(
        !scope.content_revision.trim().is_empty(),
        "localization scope {} has no content revision",
        scope.id
    );

    let mut ids = BTreeSet::new();
    let mut localized_units = 0;
    let mut source_preserved_units = 0;
    let mut unresolved_units = Vec::new();

    for unit in &scope.units {
        ensure!(!unit.id.trim().is_empty(), "localization unit id is empty");
        ensure!(
            ids.insert(unit.id.as_str()),
            "duplicate localization unit id {}",
            unit.id
        );
        ensure!(
            !(unit.disposition == BuildDisposition::UseLocalized
                && unit.review_state == ReviewState::Untranslated),
            "localization unit {} selects localized text but is untranslated",
            unit.id
        );

        match unit.disposition {
            BuildDisposition::PreserveSource => source_preserved_units += 1,
            BuildDisposition::UseLocalized => localized_units += 1,
        }
        if unit.disposition != BuildDisposition::UseLocalized
            || unit.review_state != ReviewState::Complete
        {
            unresolved_units.push(unit.id.clone());
        }
    }

    if mode == BuildMode::ReleaseCandidate {
        ensure!(
            scope.release_approval == ReleaseApproval::Approved,
            "localization scope {} lacks release approval",
            scope.id
        );
        ensure!(
            scope.approved_revision.as_deref() == Some(scope.content_revision.as_str()),
            "localization scope {} changed after release approval",
            scope.id
        );
        ensure!(
            unresolved_units.is_empty(),
            "release candidate scope {} has unresolved units: {}",
            scope.id,
            unresolved_units.join(", ")
        );
    }

    Ok(ReadinessReport {
        scope_id: scope.id.clone(),
        mode,
        release_candidate: mode == BuildMode::ReleaseCandidate,
        localized_units,
        source_preserved_units,
        unresolved_units,
    })
}

#[cfg(test)]
#[path = "build_mode_tests.rs"]
mod tests;
