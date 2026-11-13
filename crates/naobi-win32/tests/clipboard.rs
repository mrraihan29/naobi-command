//! Tier 2 (interactive desktop): writes the real clipboard once, then reads it back the way the
//! history does. It replaces whatever the Owner had copied, so run it with the dogfood instance quit.
use naobi_win32::{clipboard, clipboard_marker};

// @D-019
#[test]
#[ignore = "tier2: interactive desktop"] // naobi-allow: tier 2 test, D-025
fn d019_snapshot_reads_text_and_sees_the_private_marker() {
    let before = clipboard::sequence();
    clipboard_marker::write_text("naobi-test snapshot text").expect("write");
    assert_ne!(
        clipboard::sequence(),
        before,
        "the sequence number moves on a write"
    );
    let s = clipboard::read().expect("read");
    assert_eq!(s.text.as_deref(), Some("naobi-test snapshot text"));
    assert!(s.marker, "our own write carries the marker");
    assert!(!s.exclude_from_monitor);
    assert_eq!(s.can_include_in_history, None);
    assert_eq!(s.can_upload_to_cloud, None);
    assert!(s.files.is_empty() && s.dib.is_none());
    // An own write has no owner window, so there is no exe to name.
    assert_eq!(s.owner_exe, None);
}
