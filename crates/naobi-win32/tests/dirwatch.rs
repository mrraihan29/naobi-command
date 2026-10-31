//! Tier 2 (interactive desktop): a real directory watch on a temp folder. It touches nothing else.
use std::time::Duration;

use naobi_win32::dirwatch::{DirWatcher, Wake};

#[test]
#[ignore = "tier2: interactive desktop"] // naobi-allow: tier 2 test, D-025
fn a_new_file_wakes_the_watch_and_a_quiet_folder_times_out() {
    let dir = std::env::temp_dir().join(format!("naobi-dirwatch-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("temp dir");
    let mut watch = DirWatcher::open(&dir).expect("open the watch");
    assert_eq!(
        watch.wait(Some(Duration::from_millis(100))).expect("wait"),
        Wake::Timeout,
        "nothing changed yet"
    );
    std::fs::write(dir.join("settings.toml"), "[general]\n").expect("write");
    assert_eq!(
        watch.wait(Some(Duration::from_secs(5))).expect("wait"),
        Wake::Changed
    );
    drop(watch);
    let _ = std::fs::remove_dir_all(&dir);
}
