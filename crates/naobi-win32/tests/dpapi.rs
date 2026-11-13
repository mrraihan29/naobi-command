//! Tier 2 (interactive desktop): the real DPAPI for the current user.
use naobi_win32::dpapi::{protect, unprotect};

// @R-009
#[test]
#[ignore = "tier2: interactive desktop"] // naobi-allow: tier 2 test, D-025
fn r009_dpapi_round_trip_and_the_blob_hides_the_plaintext() {
    let secret = b"zq-dpapi-marker-90417 some clipboard text".to_vec();
    let blob = protect(&secret).expect("protect");
    assert!(blob.len() > secret.len(), "a blob carries a header");
    assert!(
        !blob.windows(12).any(|w| w == &secret[..12]),
        "the plaintext is visible in the blob"
    );
    assert_eq!(unprotect(&blob).expect("unprotect"), secret);
    let mut tampered = blob.clone();
    let last = tampered.len() - 1;
    tampered[last] ^= 0xFF;
    assert!(
        unprotect(&tampered).is_err(),
        "a changed blob must not open"
    );
    assert!(unprotect(b"not a blob").is_err());
    assert_eq!(
        unprotect(&protect(b"").expect("protect empty")).expect("open"),
        b""
    );
}
