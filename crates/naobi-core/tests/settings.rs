use naobi_core::chord::{KeyChord, ALT, CTRL, SHIFT, WIN};
use naobi_core::event::AppEvent;
use naobi_core::ids::TimerId;
use naobi_core::paths;
use naobi_core::settings::Settings;
use std::ffi::OsString;
use std::path::PathBuf;

fn chord(mods: u8, vk: u32) -> KeyChord {
    KeyChord { mods, vk }
}

// @D-024
#[test]
fn d024_default_quicklink_aliases() {
    let p = Settings::parse("").unwrap();
    assert!(p.reports.is_empty());
    let s = p.settings;
    let aliases: Vec<_> = s.quicklinks.iter().map(|q| q.alias.as_str()).collect();
    assert_eq!(aliases, ["g", "gh", "yt", "naobi"]);
    let h = &s.hotkeys;
    assert_eq!(h["launcher"], chord(CTRL, 0x20));
    assert_eq!(h["clipboard"], chord(ALT | SHIFT, b'V'.into()));
    assert_eq!(h["color_picker"], chord(WIN | SHIFT, b'C'.into()));
    assert_eq!(h["topmost"], chord(WIN | CTRL, b'T'.into()));
    assert_eq!(s.ui.text_scale, 1.0);
    assert_eq!(s.clipboard.retention_days, 30);
    assert_eq!(s.clipboard.max_entries, 1000);
    assert!(s.clipboard.ignored_apps.contains(&"ProtonPass.exe".into()));
    assert!(s.aot.border);
    assert_eq!(s.aot.border_color, "auto");
}

#[test]
fn default_toml_round_trips_to_defaults() {
    let p = Settings::parse(Settings::default_toml()).unwrap();
    assert!(p.reports.is_empty());
    assert_eq!(p.settings, Settings::default());
    // An empty template would also parse to defaults, so check it is a real file.
    let t = Settings::default_toml();
    for k in p.settings.hotkeys.keys() {
        assert!(t.contains(&format!("{k} = ")), "{k} missing");
    }
    for q in &p.settings.quicklinks {
        assert!(t.contains(&q.url), "{} missing", q.alias);
    }
}

// @D-012
#[test]
fn d012_unknown_or_modifierless_hotkey_invalidates_file() {
    let prev = Settings::default();
    for bad in [r#"laucher = "Alt+Space""#, r#"launcher = "A""#] {
        let text = format!("[hotkeys]\n{bad}\n");
        assert!(Settings::parse(&text).is_err(), "{bad}");
        assert_eq!(Settings::apply(&prev, &text).settings, prev);
    }
}

// @D-009
#[test]
fn d009_size_cap_boundary_and_multibyte_error_line() {
    let at_cap = format!("#{}", "x".repeat(256 * 1024 - 1));
    assert!(Settings::parse(&at_cap).is_ok());
    assert!(Settings::parse(&format!("{at_cap}x")).is_err());
    let p = Settings::apply(
        &Settings::default(),
        "# \u{e9}\u{4e2d}\n[window]\ngap = = 1\n",
    );
    assert_eq!(p.reports[0].line, Some(3));
}

#[test]
fn border_color_hex_is_validated() {
    let ok = Settings::parse("[aot]\nborder_color = \"#A1b2C3\"\n").unwrap();
    assert_eq!(ok.settings.aot.border_color, "#A1b2C3");
    for bad in ["#GGGGGG", "A1B2C3", "#12345"] {
        let p = Settings::parse(&format!("[aot]\nborder_color = \"{bad}\"\n")).unwrap();
        assert_eq!(p.settings.aot.border_color, "auto");
        assert_eq!(p.reports.len(), 1);
    }
}

// @D-009
#[test]
fn d009_invalid_settings_keeps_last_valid() {
    let prev = Settings::parse("[aot]\nborder = false\n").unwrap().settings;
    let p = Settings::apply(&prev, "[aot]\nborder = true\nbroken = = 2\n");
    assert_eq!(p.settings, prev);
    assert_eq!(p.reports.len(), 1);
    assert_eq!(p.reports[0].line, Some(3));
    // Unknown keys are strict too.
    let p = Settings::apply(&prev, "[aot]\nunknown = 1\n");
    assert_eq!(p.settings, prev);
    assert_eq!(p.reports[0].line, Some(2));
}

// @D-009
#[test]
fn d009_oversize_settings_rejected() {
    let big = format!("# {}\n", "x".repeat(256 * 1024));
    assert!(Settings::parse(&big).is_err());
    let prev = Settings::default();
    assert_eq!(Settings::apply(&prev, &big).settings, prev);
}

// @D-015
#[test]
fn d015_settings_can_lower_not_raise_retention() {
    let p = Settings::parse("[clipboard]\nretention_days = 90\nmax_entries = 5000\n").unwrap();
    assert_eq!(p.settings.clipboard.retention_days, 30);
    assert_eq!(p.settings.clipboard.max_entries, 1000);
    assert_eq!(p.reports.len(), 2);
    let p = Settings::parse("[clipboard]\nretention_days = 7\nmax_entries = 50\n").unwrap();
    assert_eq!(p.settings.clipboard.retention_days, 7);
    assert_eq!(p.settings.clipboard.max_entries, 50);
    assert!(p.reports.is_empty());
}

// @R-009
#[test]
fn r009_settings_path_is_appdata_naobicommand() {
    let r = paths::resolve(None, Some(OsString::from(r"C:\Users\x\AppData\Roaming")));
    assert_eq!(
        r,
        Some(PathBuf::from(r"C:\Users\x\AppData\Roaming").join("NaobiCommand"))
    );
    let o = paths::resolve(Some(OsString::from("tmp")), Some(OsString::from("base")));
    assert_eq!(o, Some(PathBuf::from("tmp")));
    if std::env::var_os("NAOBI_SETTINGS_DIR").is_none() && std::env::var_os("APPDATA").is_some() {
        assert!(paths::settings_dir().unwrap().ends_with("NaobiCommand"));
    }
}

// @D-012
#[test]
fn d012_chord_parse() {
    assert_eq!(
        KeyChord::parse("Ctrl+Alt+Left").unwrap(),
        chord(CTRL | ALT, 0x25)
    );
    assert_eq!(
        KeyChord::parse("win+shift+c").unwrap(),
        chord(WIN | SHIFT, 0x43)
    );
    assert_eq!(KeyChord::parse("Alt+F4").unwrap(), chord(ALT, 0x73));
    assert!(KeyChord::parse("Hyper+A").is_err());
    assert!(KeyChord::parse("Ctrl+Nope").is_err());
    assert!(KeyChord::parse("Ctrl+").is_err());
    // A bad chord in the file invalidates it.
    assert!(Settings::parse("[hotkeys]\nlauncher = \"Ctrl+Nope\"\n").is_err());
}

// @D-021
#[test]
fn d021_app_event_has_timer_variant() {
    assert_eq!(AppEvent::Timer(TimerId(7)), AppEvent::Timer(TimerId(7)));
}

#[test]
fn a_file_from_before_window_snapping_was_removed_still_loads() {
    let old = "[hotkeys]
snap_left = \"Ctrl+Alt+Left\"
center = \"Ctrl+Alt+C\"

[window]
gap = 8
";
    let p = Settings::parse(old).unwrap();
    assert!(p.reports.is_empty());
    assert_eq!(p.settings, Settings::default());
}
