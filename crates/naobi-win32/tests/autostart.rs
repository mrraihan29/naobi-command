//! Tier 2: writes HKCU. Every test uses a throwaway key under `Software\NaobiCommandTest`, never the
//! Owner's Run key, and removes that key even when an assertion fails.
use std::path::Path;

use naobi_win32::autostart::{remove_key_tree, set_enabled, value};

/// Removes the throwaway tree on drop, so a failing assert does not leave it behind.
struct Cleanup(String);

impl Drop for Cleanup {
    fn drop(&mut self) {
        remove_key_tree(&self.0);
    }
}

#[test]
#[ignore = "tier2: interactive desktop"]
fn d007_autostart_toggles_hkcu_run_value() {
    let key = format!(r"Software\NaobiCommandTest\{}", std::process::id());
    let _cleanup = Cleanup(key.clone());
    let exe = Path::new(r"C:\Apps\naobi-app.exe");

    set_enabled(&key, exe, true).expect("write the Run value");
    // Quoted, so a path with spaces cannot be read as a shorter program name.
    assert_eq!(value(&key).as_deref(), Some(r#""C:\Apps\naobi-app.exe""#));

    set_enabled(&key, exe, false).expect("remove the Run value");
    assert_eq!(value(&key), None);
}
