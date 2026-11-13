//! The Windows side of the paste (NCMD-20): what `paste::write_*` puts on the clipboard, read back the
//! way the history reads it, and the fail-closed elevation check.
use naobi_win32::{clipboard, paste};

/// A 1 x 1, 24-bit bitmap: the 40-byte header and one padded pixel row.
fn tiny_dib() -> Vec<u8> {
    let mut d = Vec::new();
    d.extend_from_slice(&40u32.to_le_bytes()); // header size
    d.extend_from_slice(&1i32.to_le_bytes()); // width
    d.extend_from_slice(&1i32.to_le_bytes()); // height
    d.extend_from_slice(&1u16.to_le_bytes()); // planes
    d.extend_from_slice(&24u16.to_le_bytes()); // bits per pixel
    d.extend_from_slice(&[0; 24]); // compression .. important colours
    d.extend_from_slice(&[10, 20, 30, 0]); // one pixel, padded to 4 bytes
    d
}

// @R-015
#[test]
fn r015_a_window_that_cannot_be_checked_counts_as_elevated() {
    // Not a window: the check cannot vouch for it, so keys are never sent (T-008).
    assert!(paste::is_elevated(0));
}

// @R-015
#[test]
#[cfg(feature = "perf")]
fn r015_own_window_is_not_elevated() {
    let Some(w) = naobi_win32::edit_window::EditWindow::open() else {
        panic!("the helper window did not open");
    };
    assert!(!paste::is_elevated(w.hwnd()));
}

// @R-015
#[test]
#[ignore = "tier2: interactive desktop"] // naobi-allow: tier 2 test, D-025
fn r015_written_files_image_and_text_read_back_with_the_marker() {
    let files = vec![
        r"C:\naobi-test\a.txt".to_owned(),
        r"C:\naobi-test\b b.txt".to_owned(),
    ];
    paste::write_files(&files).expect("write files");
    let s = clipboard::read().expect("read files");
    assert_eq!(s.files, files, "DROPFILES header and double NUL are right");
    assert!(s.marker);

    paste::write_image(b"not a real png", &tiny_dib()).expect("write image");
    let s = clipboard::read().expect("read image");
    assert!(s.dib.is_some(), "CF_DIB is there");
    assert!(s.marker);

    paste::write_text("naobi-test paste text").expect("write text");
    let s = clipboard::read().expect("read text");
    assert_eq!(s.text.as_deref(), Some("naobi-test paste text"));
    assert!(s.marker);
}
