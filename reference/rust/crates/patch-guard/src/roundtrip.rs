use anyhow::{Result, ensure};
use serde::Serialize;

use crate::source::sha256_hex;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ExactRoundTripReport {
    pub boundary_id: String,
    pub len: usize,
    pub sha256: String,
}

pub fn verify_exact_roundtrip(
    boundary_id: &str,
    original: &[u8],
    rebuilt: &[u8],
) -> Result<ExactRoundTripReport> {
    ensure!(
        !boundary_id.trim().is_empty(),
        "round-trip boundary id is empty"
    );
    ensure!(
        original.len() == rebuilt.len(),
        "round-trip boundary {boundary_id} length mismatch: expected {}, found {}",
        original.len(),
        rebuilt.len()
    );
    if let Some((offset, (expected, found))) = original
        .iter()
        .zip(rebuilt)
        .enumerate()
        .find(|(_, (expected, found))| expected != found)
    {
        ensure!(
            expected == found,
            "round-trip boundary {boundary_id} differs at offset {offset:#X}: expected {expected:#04X}, found {found:#04X}"
        );
    }

    Ok(ExactRoundTripReport {
        boundary_id: boundary_id.to_owned(),
        len: original.len(),
        sha256: sha256_hex(original),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_round_trip_checks_the_complete_boundary() {
        let original = b"header-payload-tail";
        let report = verify_exact_roundtrip("container", original, original).unwrap();
        assert_eq!(report.len, original.len());

        let mut changed_tail = original.to_vec();
        *changed_tail.last_mut().unwrap() ^= 1;
        assert!(verify_exact_roundtrip("container", original, &changed_tail).is_err());
        assert!(verify_exact_roundtrip("container", original, &original[..8]).is_err());
    }
}
