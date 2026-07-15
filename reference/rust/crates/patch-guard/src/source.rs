use anyhow::{Result, ensure};
use sha2::{Digest, Sha256};

#[derive(Debug, Clone, Copy)]
pub struct SourceSpec<'a> {
    pub id: &'a str,
    pub len: usize,
    pub sha256: &'a str,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifiedSource {
    pub id: String,
    pub len: usize,
    pub sha256: String,
}

#[must_use]
pub fn sha256_hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

pub fn verify_source(spec: SourceSpec<'_>, bytes: &[u8]) -> Result<VerifiedSource> {
    ensure!(!spec.id.trim().is_empty(), "source id is empty");
    ensure!(
        bytes.len() == spec.len,
        "source {} length mismatch: expected {}, found {}",
        spec.id,
        spec.len,
        bytes.len()
    );
    let actual = sha256_hex(bytes);
    ensure!(
        actual == spec.sha256,
        "source {} SHA-256 mismatch: expected {}, found {}",
        spec.id,
        spec.sha256,
        actual
    );
    Ok(VerifiedSource {
        id: spec.id.to_owned(),
        len: bytes.len(),
        sha256: actual,
    })
}

#[cfg(test)]
#[path = "source_tests.rs"]
mod tests;
