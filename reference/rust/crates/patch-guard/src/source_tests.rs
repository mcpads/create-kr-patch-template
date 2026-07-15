use super::*;

#[test]
fn source_identity_requires_length_and_hash() {
    let bytes = b"self-authored fixture";
    let hash = sha256_hex(bytes);
    let valid = SourceSpec {
        id: "fixture-v1",
        len: bytes.len(),
        sha256: &hash,
    };
    assert_eq!(verify_source(valid, bytes).unwrap().sha256, hash);

    assert!(
        verify_source(
            SourceSpec {
                len: bytes.len() + 1,
                ..valid
            },
            bytes
        )
        .is_err()
    );
    assert!(
        verify_source(
            SourceSpec {
                sha256: "00",
                ..valid
            },
            bytes
        )
        .is_err()
    );
}
