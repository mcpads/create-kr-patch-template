use std::collections::BTreeSet;

use anyhow::{Context, Result, bail, ensure};
use patch_guard::{
    BuildDisposition, LocalizationScope, LocalizationUnit, PopulationStatus, ReleaseApproval,
    ReviewState, sha256_hex,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TranslationAsset {
    pub schema_version: u32,
    pub asset_id: String,
    pub source: TranslationSource,
    pub release_approval: AssetApproval,
    pub entries: Vec<TranslationEntry>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TranslationSource {
    pub id: String,
    pub sha256: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AssetApproval {
    pub decision: ApprovalDecision,
    pub approved_by: Option<String>,
    pub reviewed_scope_sha256: Option<String>,
    pub scope: String,
    pub basis: String,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalDecision {
    Pending,
    Approved,
    Rejected,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TranslationEntry {
    pub id: String,
    pub raw_hex: String,
    pub source_text: String,
    pub ko: String,
    pub review_state: AssetReviewState,
    pub build: AssetDisposition,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AssetReviewState {
    Untranslated,
    Draft,
    NeedsReview,
    NeedsHumanReview,
    Complete,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AssetDisposition {
    PreserveSource,
    UseLocalized,
}

impl TranslationAsset {
    pub fn from_slice(bytes: &[u8]) -> Result<Self> {
        serde_json::from_slice(bytes).context("parse synthetic translation asset")
    }

    pub fn validate_identity(&self, source_id: &str, source_sha256: &str) -> Result<()> {
        ensure!(
            self.schema_version == 1,
            "unsupported translation schema version {}",
            self.schema_version
        );
        ensure!(
            !self.asset_id.trim().is_empty(),
            "translation asset id is empty"
        );
        ensure!(
            self.source.id == source_id,
            "translation source id does not match the current source"
        );
        ensure!(
            self.source.sha256 == source_sha256,
            "translation source hash does not match the current source"
        );
        ensure!(
            !self.release_approval.scope.trim().is_empty(),
            "translation approval scope is empty"
        );
        ensure!(
            !self.release_approval.basis.trim().is_empty(),
            "translation approval basis is empty"
        );
        if self.release_approval.decision == ApprovalDecision::Approved {
            ensure!(
                self.release_approval
                    .approved_by
                    .as_deref()
                    .is_some_and(|value| !value.trim().is_empty()),
                "approved translation asset lacks an approver"
            );
            let reviewed_scope_sha256 = self
                .release_approval
                .reviewed_scope_sha256
                .as_deref()
                .unwrap_or_default();
            ensure!(
                reviewed_scope_sha256.len() == 64
                    && reviewed_scope_sha256
                        .bytes()
                        .all(|byte| byte.is_ascii_hexdigit()),
                "approved translation asset lacks a valid reviewed scope SHA-256"
            );
        }
        ensure!(!self.entries.is_empty(), "translation asset has no entries");
        let mut ids = BTreeSet::new();
        for entry in &self.entries {
            ensure!(!entry.id.trim().is_empty(), "translation entry id is empty");
            ensure!(
                ids.insert(entry.id.as_str()),
                "duplicate translation entry id {}",
                entry.id
            );
            ensure!(
                !entry.source_text.is_empty(),
                "translation entry {} has empty source text",
                entry.id
            );
            entry.raw_bytes()?;
        }
        Ok(())
    }

    pub fn localization_scope(
        &self,
        population_status: PopulationStatus,
        known_unit_ids: &[&str],
    ) -> Result<LocalizationScope> {
        Ok(LocalizationScope {
            id: self.release_approval.scope.clone(),
            content_revision: self.content_revision()?,
            population_status,
            known_unit_ids: known_unit_ids.iter().map(|id| (*id).to_owned()).collect(),
            release_approval: match self.release_approval.decision {
                ApprovalDecision::Pending => ReleaseApproval::Pending,
                ApprovalDecision::Approved => ReleaseApproval::Approved,
                ApprovalDecision::Rejected => ReleaseApproval::Rejected,
            },
            approved_revision: self.release_approval.reviewed_scope_sha256.clone(),
            units: self
                .entries
                .iter()
                .map(|entry| LocalizationUnit {
                    id: entry.id.clone(),
                    disposition: match entry.build {
                        AssetDisposition::PreserveSource => BuildDisposition::PreserveSource,
                        AssetDisposition::UseLocalized => BuildDisposition::UseLocalized,
                    },
                    review_state: match entry.review_state {
                        AssetReviewState::Untranslated => ReviewState::Untranslated,
                        AssetReviewState::Draft => ReviewState::Draft,
                        AssetReviewState::NeedsReview => ReviewState::NeedsReview,
                        AssetReviewState::NeedsHumanReview => ReviewState::NeedsHumanReview,
                        AssetReviewState::Complete => ReviewState::Complete,
                    },
                })
                .collect(),
        })
    }

    fn content_revision(&self) -> Result<String> {
        #[derive(Serialize)]
        struct ReviewSnapshot<'a> {
            source: &'a TranslationSource,
            scope: &'a str,
            entries: &'a [TranslationEntry],
        }

        let bytes = serde_json::to_vec(&ReviewSnapshot {
            source: &self.source,
            scope: &self.release_approval.scope,
            entries: &self.entries,
        })
        .context("serialize translation review snapshot")?;
        Ok(sha256_hex(&bytes))
    }
}

impl TranslationEntry {
    pub fn raw_bytes(&self) -> Result<Vec<u8>> {
        parse_hex(&self.raw_hex)
            .with_context(|| format!("parse raw bytes for translation entry {}", self.id))
    }
}

fn parse_hex(input: &str) -> Result<Vec<u8>> {
    let compact: String = input
        .chars()
        .filter(|character| !character.is_whitespace())
        .collect();
    ensure!(
        !compact.is_empty() && compact.len() % 2 == 0,
        "hex string must contain an even number of digits"
    );
    (0..compact.len())
        .step_by(2)
        .map(|index| {
            u8::from_str_radix(&compact[index..index + 2], 16)
                .with_context(|| format!("invalid hex at digit {index}"))
        })
        .collect()
}

pub fn control_tokens(text: &str) -> Result<Vec<String>> {
    let chars: Vec<char> = text.chars().collect();
    let mut tokens = Vec::new();
    let mut index = 0;
    while index < chars.len() {
        match chars[index] {
            '{' => {
                let start = index;
                index += 1;
                while index < chars.len() && chars[index] != '}' {
                    ensure!(chars[index] != '{', "nested control token");
                    index += 1;
                }
                ensure!(index < chars.len(), "unterminated control token");
                tokens.push(chars[start..=index].iter().collect());
            }
            '}' => bail!("unmatched control token terminator"),
            _ => {}
        }
        index += 1;
    }
    Ok(tokens)
}
