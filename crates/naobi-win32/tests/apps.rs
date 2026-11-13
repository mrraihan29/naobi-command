//! Tier 2 (interactive desktop): the real `AppsFolder`. It only reads the shell's app list.
use naobi_win32::apps;

// @R-012
#[test]
#[ignore = "tier2: interactive desktop"] // naobi-allow: tier 2 test, D-025
fn r012_app_catalog_lists_start_menu_apps() {
    let apps = apps::list().expect("scan AppsFolder");
    assert!(apps.len() > 5, "only {} apps", apps.len());
    assert!(
        apps.iter()
            .any(|a| a.name.to_ascii_lowercase().contains("notepad")),
        "Notepad is not in the catalog"
    );
    assert!(apps.iter().all(|a| !a.id.is_empty() && !a.name.is_empty()));
    let mut ids: Vec<_> = apps.iter().map(|a| a.id.as_str()).collect();
    let n = ids.len();
    ids.sort_unstable();
    ids.dedup();
    assert!(
        n - ids.len() <= n / 20,
        "too many duplicate ids: {}",
        n - ids.len()
    );
}
