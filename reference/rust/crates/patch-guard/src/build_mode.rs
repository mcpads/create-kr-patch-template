use std::collections::BTreeSet;

use anyhow::{Result, ensure};
use serde::Serialize;

use crate::source::sha256_hex;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BuildMode {
    Development,
    ReleaseCandidate,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PopulationStatus {
    InProgress,
    Confirmed,
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
    pub population_status: PopulationStatus,
    pub known_unit_ids: Vec<String>,
    pub release_approval: ReleaseApproval,
    pub approved_revision: Option<String>,
    pub units: Vec<LocalizationUnit>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ReadinessReport {
    pub scope_id: String,
    pub mode: BuildMode,
    pub release_candidate: bool,
    pub population_status: PopulationStatus,
    pub known_population_units: usize,
    pub review_revision: String,
    pub localized_units: usize,
    pub source_preserved_units: usize,
    pub unresolved_units: Vec<String>,
}

pub fn evaluate_readiness(mode: BuildMode, scope: &LocalizationScope) -> Result<ReadinessReport> {
    validate_scope_identity(scope)?;
    let known_unit_ids = validated_known_unit_ids(scope)?;
    let current_review_revision = review_revision_from(scope, &known_unit_ids);

    ensure!(
        !scope.units.is_empty(),
        "localization scope {} has no units",
        scope.id
    );

    let mut unit_ids = BTreeSet::new();
    let mut localized_units = 0;
    let mut source_preserved_units = 0;
    let mut unresolved_units = Vec::new();

    for unit in &scope.units {
        ensure!(!unit.id.trim().is_empty(), "localization unit id is empty");
        ensure!(
            unit_ids.insert(unit.id.as_str()),
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

    let missing: Vec<_> = known_unit_ids.difference(&unit_ids).copied().collect();
    let unexpected: Vec<_> = unit_ids.difference(&known_unit_ids).copied().collect();
    ensure!(
        missing.is_empty() && unexpected.is_empty(),
        "localization scope {} does not match its known population; missing: [{}]; unexpected: [{}]",
        scope.id,
        missing.join(", "),
        unexpected.join(", ")
    );

    if mode == BuildMode::ReleaseCandidate {
        ensure!(
            scope.population_status == PopulationStatus::Confirmed,
            "release candidate scope {} has an unconfirmed population",
            scope.id
        );
        ensure!(
            scope.release_approval == ReleaseApproval::Approved,
            "localization scope {} lacks release approval",
            scope.id
        );
        ensure!(
            scope.approved_revision.as_deref() == Some(current_review_revision.as_str()),
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
        population_status: scope.population_status,
        known_population_units: known_unit_ids.len(),
        review_revision: current_review_revision,
        localized_units,
        source_preserved_units,
        unresolved_units,
    })
}

pub fn review_revision(scope: &LocalizationScope) -> Result<String> {
    validate_scope_identity(scope)?;
    let known_unit_ids = validated_known_unit_ids(scope)?;
    Ok(review_revision_from(scope, &known_unit_ids))
}

fn validate_scope_identity(scope: &LocalizationScope) -> Result<()> {
    ensure!(
        !scope.id.trim().is_empty(),
        "localization scope id is empty"
    );
    ensure!(
        !scope.content_revision.trim().is_empty(),
        "localization scope {} has no content revision",
        scope.id
    );
    Ok(())
}

fn validated_known_unit_ids(scope: &LocalizationScope) -> Result<BTreeSet<&str>> {
    ensure!(
        !scope.known_unit_ids.is_empty(),
        "localization scope {} has no known population",
        scope.id
    );
    let mut ids = BTreeSet::new();
    for id in &scope.known_unit_ids {
        ensure!(!id.trim().is_empty(), "known localization unit id is empty");
        ensure!(
            ids.insert(id.as_str()),
            "duplicate known localization unit id {}",
            id
        );
    }
    Ok(ids)
}

fn review_revision_from(scope: &LocalizationScope, known_unit_ids: &BTreeSet<&str>) -> String {
    let mut snapshot = Vec::new();
    append_component(&mut snapshot, "patch-guard.localization-review.v1");
    append_component(&mut snapshot, &scope.id);
    append_component(&mut snapshot, &scope.content_revision);
    append_component(
        &mut snapshot,
        match scope.population_status {
            PopulationStatus::InProgress => "in_progress",
            PopulationStatus::Confirmed => "confirmed",
        },
    );
    for id in known_unit_ids {
        append_component(&mut snapshot, id);
    }
    sha256_hex(&snapshot)
}

fn append_component(snapshot: &mut Vec<u8>, value: &str) {
    snapshot.extend_from_slice(&(value.len() as u64).to_le_bytes());
    snapshot.extend_from_slice(value.as_bytes());
}

#[cfg(test)]
#[path = "build_mode_tests.rs"]
mod tests;
