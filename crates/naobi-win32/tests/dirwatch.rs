//! Tier 2 (interactive desktop): a real directory watch on a temp folder. It touches nothing else.
use std::time::Duration;

use naobi_win32::dirwatch::{
    is_local_dir, parse_notify_buffer, ChangeBatch, DirWatcher, FileAction, FileChange, Wake,
};
use std::path::{Path, PathBuf};

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

fn push_record(buf: &mut Vec<u32>, action: u32, name: &str, has_next: bool) {
    let utf16: Vec<u16> = name.encode_utf16().collect();
    let name_bytes = utf16.len() * 2;
    let total_bytes = 12 + name_bytes;
    let padded_words = total_bytes.div_ceil(4);
    let next_offset = if has_next {
        (padded_words * 4) as u32
    } else {
        0
    };
    buf.push(next_offset);
    buf.push(action);
    buf.push(name_bytes as u32);
    for pair in utf16.chunks(2) {
        let lo = u32::from(pair[0]);
        let hi = u32::from(pair.get(1).copied().unwrap_or(0));
        buf.push(lo | (hi << 16));
    }
}

// @R-013
#[test]
fn r013_parse_notify_buffer_decodes_records_and_overflow() {
    assert_eq!(
        parse_notify_buffer(&[], 0),
        ChangeBatch {
            changes: Vec::new(),
            overflow: true,
        }
    );

    let mut words = Vec::new();
    push_record(&mut words, 1, "docs\\a.txt", true);
    push_record(&mut words, 4, "old.txt", true);
    push_record(&mut words, 5, "new.txt", true);
    push_record(&mut words, 2, "gone.txt", false);
    let bytes = words.len() * 4;

    assert_eq!(
        parse_notify_buffer(&words, bytes),
        ChangeBatch {
            changes: vec![
                FileChange {
                    action: FileAction::Added,
                    rel_path: PathBuf::from("docs\\a.txt"),
                },
                FileChange {
                    action: FileAction::RenamedOld,
                    rel_path: PathBuf::from("old.txt"),
                },
                FileChange {
                    action: FileAction::RenamedNew,
                    rel_path: PathBuf::from("new.txt"),
                },
                FileChange {
                    action: FileAction::Removed,
                    rel_path: PathBuf::from("gone.txt"),
                },
            ],
            overflow: false,
        }
    );
}

// @R-013
#[test]
fn r013_is_local_dir_rejects_unc_and_accepts_system_drive() {
    assert!(!is_local_dir(Path::new(r"\\server\share\docs")));
    assert!(!is_local_dir(Path::new(r"\\?\UNC\server\share")));
    assert!(!is_local_dir(Path::new("relative\\path")));
    assert!(is_local_dir(&std::env::temp_dir()));
}
