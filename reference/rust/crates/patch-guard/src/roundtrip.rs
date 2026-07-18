use anyhow::{Result, ensure};

pub fn verify_exact_roundtrip(boundary_id: &str, original: &[u8], rebuilt: &[u8]) -> Result<()> {
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

    Ok(())
}

#[cfg(test)]
#[path = "roundtrip_tests.rs"]
mod tests;
