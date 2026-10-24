//! Tier 2 (interactive desktop): writes the real clipboard. Run with the dogfood instance quit.
use naobi_win32::clipboard_marker::{has_marker, read_text, write_text};

#[test]
#[ignore = "tier2: interactive desktop"]
fn d019_marker_text_write_roundtrip() {
    write_text("naobi marker round trip").expect("write the clipboard");
    assert!(has_marker(), "the private marker format is present");
    assert_eq!(read_text().as_deref(), Some("naobi marker round trip"));
}
