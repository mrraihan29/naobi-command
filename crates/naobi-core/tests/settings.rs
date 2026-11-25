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

// @D-043
#[test]
fn d043_slot_write_keeps_every_other_line() {
    let text = "# mine\r\n[ui]\r\ntext_scale = 1.25\r\n\r\n[theme]\r\nlight = \"paper\" # day\r\ndark = \"stone\"\r\n";
    let out = naobi_core::settings::with_theme_slot(text, "light", "harbor").expect("written");
    assert_eq!(
        out,
        "# mine\r\n[ui]\r\ntext_scale = 1.25\r\n\r\n[theme]\r\nlight = \"harbor\" # day\r\ndark = \"stone\"\r\n"
    );
    let added = naobi_core::settings::with_theme_slot("[ui]\ntext_scale = 1.0\n", "dark", "night")
        .expect("written");
    let parsed = naobi_core::settings::Settings::parse(&added).expect("valid");
    assert_eq!(parsed.settings.theme.dark, "night");
    assert_eq!(parsed.settings.ui.text_scale, 1.0);
}

// @D-043
#[test]
fn d043_slot_write_refuses_a_broken_file() {
    use naobi_core::settings::with_theme_slot;
    assert!(with_theme_slot("[ui\n", "light", "harbor").is_err());
    assert!(with_theme_slot("", "light", r"..\x").is_err());
    assert!(with_theme_slot("", "other", "harbor").is_err());
    // Either a bad id or a bad slot alone is refused before anything is written.
    for (slot, id) in [("other", "harbor"), ("light", r"..\x")] {
        match with_theme_slot("", slot, id) {
            Err(naobi_core::error::SettingsError::Syntax { message, .. }) => {
                assert_eq!(message, "that is not a theme name")
            }
            other => panic!("{slot} {id}: {other:?}"),
        }
    }
}

// @D-043
#[test]
fn d043_slot_write_finds_the_key_when_another_section_follows() {
    use naobi_core::settings::with_theme_slot;
    for (text, want) in [
        (
            "[theme]\nlight = \"paper\"\ndark = \"stone\"\n\n[ui]\ntext_scale = 1.0\n",
            "[theme]\nlight = \"paper\"\ndark = \"night\"\n\n[ui]\ntext_scale = 1.0\n",
        ),
        (
            "# top\n[theme]\nlight = \"paper\"\ndark = \"stone\"\n[ui]\ntext_scale = 1.0\n",
            "# top\n[theme]\nlight = \"paper\"\ndark = \"night\"\n[ui]\ntext_scale = 1.0\n",
        ),
    ] {
        assert_eq!(
            with_theme_slot(text, "dark", "night").expect("written"),
            want
        );
    }
    // A missing key goes right under the [theme] header, not into the section above it.
    let out = with_theme_slot(
        "[ui]\ntext_scale = 1.0\n[theme]\nlight = \"paper\"\n",
        "dark",
        "night",
    )
    .expect("written");
    assert_eq!(
        out,
        "[ui]\ntext_scale = 1.0\n[theme]\ndark = \"night\"\nlight = \"paper\"\n"
    );
}

// @D-043
#[test]
fn d043_a_theme_file_is_capped_at_64_kib() {
    assert_eq!(naobi_core::theme_file::MAX_THEME_BYTES, 65_536);
}

#[test]
fn calculator_rem_base_px_defaults_to_16_and_validates() {
    let def = Settings::parse("").unwrap();
    assert_eq!(def.settings.calculator.rem_base_px, 16.0);
    assert!(def.reports.is_empty());

    let custom = Settings::parse("[calculator]\nrem_base_px = 10.0\n").unwrap();
    assert_eq!(custom.settings.calculator.rem_base_px, 10.0);
    assert!(custom.reports.is_empty());

    for bad in ["0.0", "-8.0", "nan", "inf"] {
        let p = Settings::parse(&format!("[calculator]\nrem_base_px = {bad}\n")).unwrap();
        assert_eq!(p.settings.calculator.rem_base_px, 16.0, "{bad}");
        assert_eq!(p.reports.len(), 1, "{bad}");
    }

    assert!(Settings::parse("[calculator]\nunknown = 1\n").is_err());
}

#[test]
fn currency_settings_default_validate_and_fallback_with_report() {
    let def = Settings::parse("").unwrap();
    assert_eq!(def.settings.currency.default_currency, "IDR");
    assert_eq!(def.settings.currency.number_format, "us");
    assert!(def.settings.currency.auto_refresh);
    assert!(def.reports.is_empty());
    assert!(Settings::default_toml().contains("[currency]"));

    let custom = Settings::parse(
        "[currency]\ndefault_currency = \"eur\"\nnumber_format = \"id\"\nauto_refresh = false\n",
    )
    .unwrap();
    assert_eq!(custom.settings.currency.default_currency, "EUR");
    assert_eq!(custom.settings.currency.number_format, "id");
    assert!(!custom.settings.currency.auto_refresh);
    assert!(custom.reports.is_empty());

    for bad in ["", "US", "USDD", "123", "usd1"] {
        let p = Settings::parse(&format!("[currency]\ndefault_currency = \"{bad}\"\n")).unwrap();
        assert_eq!(p.settings.currency.default_currency, "IDR", "{bad}");
        assert_eq!(p.reports.len(), 1, "{bad}");
    }

    for bad in ["", "eu", "de"] {
        let p = Settings::parse(&format!("[currency]\nnumber_format = \"{bad}\"\n")).unwrap();
        assert_eq!(p.settings.currency.number_format, "us", "{bad}");
        assert_eq!(p.reports.len(), 1, "{bad}");
    }

    assert!(Settings::parse("[currency]\nunknown = 1\n").is_err());
}

#[test]
fn window_switcher_hotkey_is_optional_with_no_default_binding() {
    let def = Settings::parse("").unwrap();
    assert!(!def.settings.hotkeys.contains_key("window_switcher"));
    assert!(!Settings::default().hotkeys.contains_key("window_switcher"));

    let bound = Settings::parse("[hotkeys]\nwindow_switcher = \"Alt+Shift+W\"\n").unwrap();
    assert!(bound.reports.is_empty());
    assert_eq!(
        bound.settings.hotkeys.get("window_switcher"),
        Some(&chord(ALT | SHIFT, b'W'.into()))
    );

    assert!(Settings::parse("[hotkeys]\nwindow_switcher = \"W\"\n").is_err());
}

#[test]
fn emoji_hotkey_is_optional_with_no_default_binding() {
    let def = Settings::parse("").unwrap();
    assert!(!def.settings.hotkeys.contains_key("emoji"));
    assert!(!Settings::default().hotkeys.contains_key("emoji"));

    let bound = Settings::parse("[hotkeys]\nemoji = \"Alt+Shift+E\"\n").unwrap();
    assert!(bound.reports.is_empty());
    assert_eq!(
        bound.settings.hotkeys.get("emoji"),
        Some(&chord(ALT | SHIFT, b'E'.into()))
    );

    assert!(Settings::parse("[hotkeys]\nemoji = \"E\"\n").is_err());
}

#[test]
fn snippets_hotkey_is_optional_with_no_default_binding() {
    let def = Settings::parse("").unwrap();
    assert!(!def.settings.hotkeys.contains_key("snippets"));
    assert!(!Settings::default().hotkeys.contains_key("snippets"));

    let bound = Settings::parse("[hotkeys]\nsnippets = \"Alt+Shift+S\"\n").unwrap();
    assert!(bound.reports.is_empty());
    assert_eq!(
        bound.settings.hotkeys.get("snippets"),
        Some(&chord(ALT | SHIFT, b'S'.into()))
    );

    assert!(Settings::parse("[hotkeys]\nsnippets = \"S\"\n").is_err());
}

#[test]
fn scratchpad_hotkey_is_optional_with_no_default_binding() {
    let def = Settings::parse("").unwrap();
    assert!(!def.settings.hotkeys.contains_key("scratchpad"));
    assert!(!Settings::default().hotkeys.contains_key("scratchpad"));

    let bound = Settings::parse("[hotkeys]\nscratchpad = \"Alt+Shift+N\"\n").unwrap();
    assert!(bound.reports.is_empty());
    assert_eq!(
        bound.settings.hotkeys.get("scratchpad"),
        Some(&chord(ALT | SHIFT, b'N'.into()))
    );

    assert!(Settings::parse("[hotkeys]\nscratchpad = \"N\"\n").is_err());
}

// @R-013
#[test]
fn r013_files_settings_default_and_custom_parse_strictly() {
    let def = Settings::parse("").unwrap();
    assert!(def.reports.is_empty());
    assert!(def.settings.files.roots.is_empty());
    assert_eq!(
        def.settings.files.ignore,
        [
            ".git",
            "node_modules",
            "target",
            ".cargo",
            ".venv",
            "dist",
            "build"
        ]
    );

    let custom_roots =
        Settings::parse("[files]\nroots = [\"D:\\\\Work\", \"C:\\\\Notes\"]\n").unwrap();
    assert!(custom_roots.reports.is_empty());
    assert_eq!(custom_roots.settings.files.roots, ["D:\\Work", "C:\\Notes"]);
    assert_eq!(
        custom_roots.settings.files.ignore,
        def.settings.files.ignore
    );

    let custom_ignore = Settings::parse("[files]\nignore = [\".git\", \"out\"]\n").unwrap();
    assert!(custom_ignore.reports.is_empty());
    assert!(custom_ignore.settings.files.roots.is_empty());
    assert_eq!(custom_ignore.settings.files.ignore, [".git", "out"]);

    assert!(Settings::parse("[files]\nunknown = 1\n").is_err());
}

#[test]
fn item_aliases_validate_and_report_conflicts() {
    let ok = Settings::parse(
        "[aliases]\nterm = \"app:wt.exe\"\nb64 = \"dev:b64\"\n",
    )
    .unwrap();
    assert!(ok.reports.is_empty());
    assert_eq!(ok.settings.aliases.get("term").map(String::as_str), Some("app:wt.exe"));
    assert_eq!(ok.settings.aliases.get("b64").map(String::as_str), Some("dev:b64"));

    // Whitespace in alias, empty target, case-duplicate alias, or quicklink collision report and keep first.
    let bad = Settings::parse(
        "[aliases]\n\"has space\" = \"app:a\"\nempty = \"\"\nTerm = \"app:first\"\nterm = \"app:second\"\ng = \"sys.lock\"\n",
    )
    .unwrap();
    assert_eq!(bad.settings.aliases.get("Term").map(String::as_str), Some("app:first"));
    assert!(!bad.settings.aliases.contains_key("term"));
    assert!(!bad.settings.aliases.contains_key("g"));
    assert!(!bad.settings.aliases.contains_key("has space"));
    assert!(!bad.settings.aliases.contains_key("empty"));
    assert_eq!(bad.reports.len(), 4);
}
