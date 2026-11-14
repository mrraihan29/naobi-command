//! Tier 2: the real shell icons. An app's icon comes from its AppsFolder entry; a file's icon comes
//! from the extension alone, so a planted `.url` whose icon location is a network share is never
//! read (T-030).
use naobi_core::ui::Icon;
use naobi_win32::icons::{app_icon, ext_icon, with_com};
use std::time::{Duration, Instant};

fn visible(i: &Icon) -> bool {
    i.rgba.len() == (i.w * i.h * 4) as usize && i.rgba.chunks(4).any(|p| p[3] > 0)
}

// @R-012
#[test]
#[ignore = "tier2: interactive desktop"] // naobi-allow: tier 2 test, D-025
fn r012_an_installed_apps_icon_is_loaded() {
    let apps = naobi_win32::apps::list().unwrap_or_else(|e| panic!("catalog: {e}"));
    assert!(!apps.is_empty(), "no installed apps");
    let found = with_com(|| apps.iter().take(30).find_map(|a| app_icon(&a.id)));
    let icon = found.unwrap_or_else(|| panic!("no icon for any of the first 30 apps"));
    assert!(
        visible(&icon),
        "the picture is empty: {}x{}",
        icon.w,
        icon.h
    );
}

// @R-010
#[test]
#[ignore = "tier2: interactive desktop"] // naobi-allow: tier 2 test, D-025
fn r010_file_row_icon_does_not_touch_unc() {
    // A planted .url that points its icon at a share nobody answers (TEST-NET-1): reading it would
    // hang the shell for seconds, and open an SMB connection.
    let dir = std::env::temp_dir().join(format!("naobi-icons-{}", std::process::id()));
    let _ = std::fs::create_dir_all(&dir);
    let planted = dir.join("planted.url");
    let _ = std::fs::write(
        &planted,
        "[InternetShortcut]\r\nURL=https://example.com\r\nIconFile=\\\\192.0.2.1\\share\\x.ico\r\nIconIndex=0\r\n",
    );
    let started = Instant::now();
    let icons: Vec<Option<Icon>> = with_com(|| {
        ["url", "lnk", "library-ms", "searchConnector-ms", "txt", ""]
            .iter()
            .map(|e| ext_icon(e))
            .collect()
    });
    let took = started.elapsed();
    let _ = std::fs::remove_dir_all(&dir);
    assert!(
        took < Duration::from_secs(2),
        "icon lookup took {took:?}: it touched something slow"
    );
    for (i, icon) in icons.iter().enumerate() {
        let icon = icon
            .as_ref()
            .unwrap_or_else(|| panic!("no icon for kind {i}"));
        assert!(visible(icon), "kind {i} is empty");
    }
}
