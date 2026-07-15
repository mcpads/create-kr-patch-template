use std::ops::Range;

use anyhow::{Context, Result, bail, ensure};
use patch_guard::SourceSpec;

use crate::translation::{AssetDisposition, TranslationAsset, control_tokens};

pub const DEMO_SOURCE_ID: &str = "self-authored-synthetic-image-v1";
pub(super) const DEMO_OUTPUT_ID: &str = "synthetic-patched-image-v1";
pub(super) const DEMO_SOURCE_SHA256: &str =
    "e8a94f246ababf82ee6d9176a31b9415958f6adbdb3762ef861e58c7b81d68eb";
pub(super) const DEMO_SOURCE_LEN: usize = 64;
pub(super) const POINTER_RANGES: [Range<usize>; 2] = [4..6, 6..8];
pub(super) const CHECKSUM_RANGE: Range<usize> = 8..10;
pub(super) const IMMUTABLE_RANGE: Range<usize> = 10..64;
pub(super) const ORIGINAL_TEXT_RANGES: [Range<usize>; 2] = [16..22, 22..28];
pub(super) const ENTRY_IDS: [&str; 2] = ["demo.dialog.0001", "demo.dialog.0002"];
const SOURCE_TEXTS: [&str; 2] = ["HELLO{end}", "WORLD{end}"];

pub const fn source_spec() -> SourceSpec<'static> {
    SourceSpec {
        id: DEMO_SOURCE_ID,
        len: DEMO_SOURCE_LEN,
        sha256: DEMO_SOURCE_SHA256,
    }
}

pub fn demo_source() -> Vec<u8> {
    let mut source = Vec::with_capacity(DEMO_SOURCE_LEN);
    source.extend_from_slice(b"SYN1");
    source.extend_from_slice(&(ORIGINAL_TEXT_RANGES[0].start as u16).to_le_bytes());
    source.extend_from_slice(&(ORIGINAL_TEXT_RANGES[1].start as u16).to_le_bytes());
    source.extend_from_slice(&[0, 0]);
    source.extend_from_slice(b"GUARD!");
    source.extend_from_slice(b"HELLO\0");
    source.extend_from_slice(b"WORLD\0");
    source.resize(DEMO_SOURCE_LEN, 0xff);
    let checksum = checksum(&source).to_le_bytes();
    source[CHECKSUM_RANGE].copy_from_slice(&checksum);
    source
}

pub(super) fn validate_population(source: &[u8], asset: &TranslationAsset) -> Result<()> {
    ensure!(
        asset.entries.len() == ENTRY_IDS.len(),
        "synthetic translation population mismatch"
    );
    for (index, entry) in asset.entries.iter().enumerate() {
        ensure!(
            entry.id == ENTRY_IDS[index],
            "unexpected synthetic translation entry {}",
            entry.id
        );
        ensure!(
            entry.raw_bytes()? == source[ORIGINAL_TEXT_RANGES[index].clone()],
            "protected source bytes differ for {}",
            entry.id
        );
        ensure!(
            entry.source_text == SOURCE_TEXTS[index],
            "protected source text differs for {}",
            entry.id
        );
        if entry.build == AssetDisposition::UseLocalized {
            ensure!(
                !entry.ko.is_empty(),
                "selected translation {} is empty",
                entry.id
            );
            ensure!(
                control_tokens(&entry.source_text)? == control_tokens(&entry.ko)?,
                "control tokens changed for {}",
                entry.id
            );
        }
    }
    Ok(())
}

pub(super) fn verify_output(
    source: &[u8],
    output: &[u8],
    asset: &TranslationAsset,
    pointer_updates: &[(usize, u16)],
) -> Result<()> {
    ensure!(
        output[0..4] == source[0..4] && output[IMMUTABLE_RANGE] == source[IMMUTABLE_RANGE],
        "protected synthetic bytes changed"
    );
    ensure!(
        stored_checksum(output)? == checksum(output),
        "synthetic output checksum mismatch"
    );
    for (index, entry) in asset.entries.iter().enumerate() {
        let pointer =
            u16::from_le_bytes(output[POINTER_RANGES[index].clone()].try_into().unwrap()) as usize;
        if let Some((_, expected_pointer)) = pointer_updates
            .iter()
            .find(|(updated_index, _)| *updated_index == index)
        {
            let encoded = encode_demo_text(&entry.ko)?;
            ensure!(
                pointer == usize::from(*expected_pointer),
                "synthetic pointer mismatch for {}",
                entry.id
            );
            ensure!(
                output.get(pointer..pointer + encoded.len()) == Some(encoded.as_slice()),
                "synthetic encoded text mismatch for {}",
                entry.id
            );
        } else {
            ensure!(
                pointer == ORIGINAL_TEXT_RANGES[index].start,
                "source-preserved pointer changed for {}",
                entry.id
            );
        }
    }
    Ok(())
}

pub(super) fn stored_checksum(bytes: &[u8]) -> Result<u16> {
    let stored: [u8; 2] = bytes
        .get(CHECKSUM_RANGE)
        .context("synthetic checksum field is missing")?
        .try_into()
        .unwrap();
    Ok(u16::from_le_bytes(stored))
}

pub(super) fn checksum(bytes: &[u8]) -> u16 {
    bytes
        .iter()
        .enumerate()
        .filter(|(index, _)| !CHECKSUM_RANGE.contains(index))
        .fold(0_u16, |sum, (_, byte)| sum.wrapping_add(u16::from(*byte)))
}

pub(super) fn encode_demo_text(text: &str) -> Result<Vec<u8>> {
    let chars: Vec<char> = text.chars().collect();
    let mut encoded = Vec::new();
    let mut index = 0;
    while index < chars.len() {
        if chars[index] == '{' {
            let start = index;
            while index < chars.len() && chars[index] != '}' {
                index += 1;
            }
            ensure!(index < chars.len(), "unterminated control token");
            let token: String = chars[start..=index].iter().collect();
            ensure!(token == "{end}", "unsupported synthetic token {token}");
            encoded.push(0);
        } else {
            encoded.push(match chars[index] {
                '안' => 0x80,
                '녕' => 0x81,
                '세' => 0x82,
                '계' => 0x83,
                character => bail!("unsupported synthetic character {character}"),
            });
        }
        index += 1;
    }
    ensure!(
        encoded.last() == Some(&0),
        "synthetic text lacks an end token"
    );
    Ok(encoded)
}
