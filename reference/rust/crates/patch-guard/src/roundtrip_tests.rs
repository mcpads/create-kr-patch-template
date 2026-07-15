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
