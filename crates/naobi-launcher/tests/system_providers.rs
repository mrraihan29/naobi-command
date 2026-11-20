//! System commands, quicklinks and the calculator through the public engine, with a fake `SystemOps`.
use naobi_core::db::Db;
use naobi_core::effect::{Effect, LaunchTarget};
use naobi_core::quicklink::Quicklink;
use naobi_core::settings::Settings;
use naobi_core::ui::{Key, KeyChord, Screen, UiEvent};
use naobi_launcher::frecency::Frecency;
use naobi_launcher::providers::calc::CalcSource;
use naobi_launcher::providers::quicklink::{QuicklinkSet, QuicklinkSource};
use naobi_launcher::providers::system::{SystemCommand, SystemOps, SystemSource};
use naobi_launcher::{Clock, Launcher};
use std::sync::{Arc, Mutex};

struct Zero;
impl Clock for Zero {
    fn now_ms(&self) -> i64 {
        1_000_000_000_000
    }
}

/// Records every command it is asked to run; `fail` makes it answer with an error.
#[derive(Default)]
struct FakeOps {
    ran: Mutex<Vec<SystemCommand>>,
    fail: bool,
}

impl SystemOps for FakeOps {
    fn run(&self, command: SystemCommand) -> Result<(), String> {
        self.ran
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(command);
        if self.fail {
            Err("Windows refused the command.".into())
        } else {
            Ok(())
        }
    }
}

impl FakeOps {
    fn ran(&self) -> Vec<SystemCommand> {
        self.ran.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }
}

struct Rig {
    launcher: Launcher,
    ops: Arc<FakeOps>,
    links: QuicklinkSet,
}

fn rig_with(ops: FakeOps) -> Rig {
    let ops = Arc::new(ops);
    let links = QuicklinkSet::default();
    links.set(Settings::default().quicklinks);
    let db = Db::open_in_memory().unwrap_or_else(|e| panic!("db: {e}"));
    let frecency = Frecency::open(db, 0).unwrap_or_else(|e| panic!("frecency: {e}"));
    let launcher = Launcher::new(
        vec![
            Box::new(CalcSource::default()),
            Box::new(QuicklinkSource::new(links.clone())),
            Box::new(SystemSource::new(ops.clone())),
        ],
        Vec::new(),
        frecency,
        Box::new(Zero),
    );
    Rig {
        launcher,
        ops,
        links,
    }
}

fn rig() -> Rig {
    rig_with(FakeOps::default())
}

fn enter() -> UiEvent {
    UiEvent::Key(KeyChord {
        key: Key::Enter,
        ctrl: false,
        shift: false,
        alt: false,
    })
}

fn titles(s: &Screen) -> Vec<String> {
    s.sections
        .iter()
        .flat_map(|sec| sec.rows.iter().map(|r| r.title.clone()))
        .collect()
}

fn type_text(l: &mut Launcher, text: &str) {
    l.handle(UiEvent::TextChanged(text.into()));
}

// @R-020
#[test]
fn r020_single_enter_arms_second_executes() {
    let mut r = rig();
    type_text(&mut r.launcher, "restart");
    assert_eq!(titles(&r.launcher.screen())[0], "Restart");
    assert_eq!(
        r.launcher.handle(enter()),
        vec![],
        "the first Enter only arms"
    );
    assert!(r.ops.ran().is_empty(), "nothing ran yet");
    assert!(r.launcher.screen().sections[0].rows[0].armed);
    assert_eq!(
        r.launcher.screen().footer.primary.as_deref(),
        Some("Confirm")
    );
    assert_eq!(r.launcher.handle(enter()), vec![Effect::Hide]);
    assert_eq!(r.ops.ran(), vec![SystemCommand::Restart]);
}

// @R-020
#[test]
fn r020_only_the_four_destructive_commands_need_a_second_enter() {
    let armed: Vec<_> = SystemCommand::ALL
        .into_iter()
        .filter(|c| c.armed())
        .collect();
    assert_eq!(
        armed,
        vec![
            SystemCommand::Restart,
            SystemCommand::ShutDown,
            SystemCommand::SignOut,
            SystemCommand::EmptyRecycleBin
        ]
    );
    // The others run on the first Enter.
    for (text, command) in [
        ("lock", SystemCommand::Lock),
        ("hibernate", SystemCommand::Hibernate),
        ("toggle dark", SystemCommand::ToggleDarkMode),
        ("windows settings", SystemCommand::OpenWindowsSettings),
    ] {
        let mut r = rig();
        type_text(&mut r.launcher, text);
        assert_eq!(r.launcher.handle(enter()), vec![Effect::Hide], "{text}");
        assert_eq!(r.ops.ran(), vec![command], "{text}");
    }
}

// @R-020
#[test]
fn r020_a_failed_command_shows_a_notice_and_keeps_the_launcher_open() {
    let mut r = rig_with(FakeOps {
        fail: true,
        ..FakeOps::default()
    });
    type_text(&mut r.launcher, "lock");
    let effects = r.launcher.handle(enter());
    assert!(
        matches!(effects.as_slice(), [Effect::Notice(n)] if n.subject == "Lock"),
        "{effects:?}"
    );
}

// @D-024
#[test]
fn d024_catalog_contains_listed_commands_only() {
    let ids: Vec<_> = SystemCommand::ALL.iter().map(|c| c.id()).collect();
    assert_eq!(
        ids,
        vec![
            "sys.lock",
            "sys.sleep",
            "sys.hibernate",
            "sys.restart",
            "sys.shut_down",
            "sys.sign_out",
            "sys.empty_recycle_bin",
            "sys.toggle_dark_mode",
            "sys.open_windows_settings",
            "sys.open_settings_file",
            "sys.pause_hotkeys",
            "sys.quit",
        ]
    );
    // The catalog has no run-as-administrator or any phase-2 command.
    assert!(!ids
        .iter()
        .any(|id| id.contains("admin") || id.contains("elevat")));
    // The titles are what the Owner types, including the three app-level commands.
    let titles: Vec<_> = SystemCommand::ALL.iter().map(|c| c.title()).collect();
    assert!(titles.contains(&"Open Settings File") && titles.contains(&"Pause Hotkeys"));
    assert!(titles.contains(&"Quit Naobi Command"));
}

// @R-021
#[test]
fn r021_gh_react_native_opens_url_with_percent20() {
    let mut r = rig();
    type_text(&mut r.launcher, "gh react native");
    let s = r.launcher.screen();
    assert_eq!(titles(&s)[0], "Search GitHub for react native");
    assert_eq!(
        s.sections[0].rows[0].subtitle,
        "https://github.com/search?q=react%20native"
    );
    assert_eq!(
        r.launcher.handle(enter()),
        vec![
            Effect::Launch(LaunchTarget::Url(
                "https://github.com/search?q=react%20native".into()
            )),
            Effect::Hide
        ]
    );
}

// @R-021
#[test]
fn r021_an_edited_alias_works_after_the_settings_reload() {
    let mut r = rig();
    type_text(&mut r.launcher, "docs tokio");
    assert!(titles(&r.launcher.screen())
        .iter()
        .all(|t| !t.starts_with("Search")));
    r.links.set(vec![Quicklink {
        name: "Docs".into(),
        alias: "docs".into(),
        url: "https://docs.rs/releases/search?query={query}".into(),
    }]);
    r.launcher.refresh_sources();
    assert_eq!(titles(&r.launcher.screen())[0], "Search Docs for tokio");
    // The old defaults are gone with the reload.
    type_text(&mut r.launcher, "gh react");
    assert!(titles(&r.launcher.screen())
        .iter()
        .all(|t| !t.starts_with("Search")));
    // A bare alias, or an alias with nothing to search for, shows no quicklink row.
    type_text(&mut r.launcher, "docs");
    assert!(titles(&r.launcher.screen())
        .iter()
        .all(|t| !t.starts_with("Search")));
}

// @R-021
#[test]
fn r021_a_quicklink_removed_before_enter_is_a_notice_not_a_launch() {
    let mut r = rig();
    type_text(&mut r.launcher, "g tokio");
    r.links.set(Vec::new());
    let effects = r.launcher.handle(enter());
    assert!(
        matches!(effects.as_slice(), [Effect::Notice(n)] if n.subject == "Quicklink"),
        "{effects:?}"
    );
}

// @R-014
#[test]
fn r014_enter_copies_result_text() {
    let mut r = rig();
    type_text(&mut r.launcher, "2^10 + 15%");
    let s = r.launcher.screen();
    assert_eq!(titles(&s)[0], "1177.6");
    assert!(s.sections[0].rows[0].calculator);
    assert_eq!(s.sections[0].rows[0].subtitle, "2^10 + 15%");
    assert_eq!(s.footer.primary.as_deref(), Some("Copy"));
    assert_eq!(
        r.launcher.handle(enter()),
        vec![Effect::CopyText("1177.6".into()), Effect::Hide]
    );
}

// @R-014
#[test]
fn r014_calculator_row_hidden_for_bare_number() {
    let mut r = rig();
    for text in ["42", "3.14", "hello", "1/0", "7 / 0", "lock"] {
        type_text(&mut r.launcher, text);
        let s = r.launcher.screen();
        assert!(
            s.sections
                .iter()
                .flat_map(|sec| &sec.rows)
                .all(|row| !row.calculator),
            "{text} shows a calculator row"
        );
    }
}

// @R-014
#[test]
fn r014_unit_conversion_and_hex_copy_their_text() {
    let mut r = rig();
    type_text(&mut r.launcher, "255 to hex");
    assert_eq!(
        r.launcher.handle(enter()),
        vec![Effect::CopyText("0xFF".into()), Effect::Hide]
    );
}

struct Tick(Arc<std::sync::atomic::AtomicI64>);
impl Clock for Tick {
    fn now_ms(&self) -> i64 {
        self.0.load(std::sync::atomic::Ordering::SeqCst)
    }
}

// @R-020
#[test]
fn r020_a_double_click_arms_but_does_not_confirm_a_click_later_does() {
    use std::sync::atomic::{AtomicI64, Ordering};
    let ops = Arc::new(FakeOps::default());
    let now = Arc::new(AtomicI64::new(1_000_000_000_000));
    let db = Db::open_in_memory().unwrap_or_else(|e| panic!("db: {e}"));
    let frecency = Frecency::open(db, 0).unwrap_or_else(|e| panic!("frecency: {e}"));
    let mut l = Launcher::new(
        vec![Box::new(SystemSource::new(ops.clone()))],
        Vec::new(),
        frecency,
        Box::new(Tick(now.clone())),
    );
    type_text(&mut l, "restart");
    l.handle(UiEvent::RowActivated(0));
    assert!(l.screen().sections[0].rows[0].armed, "the first click arms");
    // The second click of a double click, 100 ms later: nothing runs, it stays armed.
    now.fetch_add(100, Ordering::SeqCst);
    assert_eq!(l.handle(UiEvent::RowActivated(0)), vec![]);
    assert!(ops.ran().is_empty());
    assert!(l.screen().sections[0].rows[0].armed, "still armed");
    // A deliberate second click after the guard confirms.
    now.fetch_add(500, Ordering::SeqCst);
    assert_eq!(l.handle(UiEvent::RowActivated(0)), vec![Effect::Hide]);
    assert_eq!(ops.ran(), vec![SystemCommand::Restart]);
}

// @R-020
#[test]
fn r020_a_click_while_the_action_panel_is_open_does_not_run_the_row_under_it() {
    let mut r = rig();
    type_text(&mut r.launcher, "lock");
    // Ctrl+K opens the action panel.
    r.launcher.handle(UiEvent::Key(KeyChord {
        key: Key::K,
        ctrl: true,
        shift: false,
        alt: false,
    }));
    assert!(r.launcher.screen().action_panel.is_some());
    assert_eq!(r.launcher.handle(UiEvent::RowActivated(0)), vec![]);
    assert!(r.ops.ran().is_empty());
}

// @R-014
#[test]
fn t4_timezone_date_and_epoch_copy_on_enter() {
    // 2026-11-20T17:02:32Z
    const NOW_MS: i64 = 1_795_194_152_000;
    let db = Db::open_in_memory().unwrap_or_else(|e| panic!("db: {e}"));
    let frecency = Frecency::open(db, 0).unwrap_or_else(|e| panic!("frecency: {e}"));
    let mut l = Launcher::new(
        vec![Box::new(CalcSource::with_now(|| NOW_MS))],
        Vec::new(),
        frecency,
        Box::new(Zero),
    );

    for (query, expected) in [
        ("5pm est in wib", "05:00 WIB (+1d, UTC+7)"),
        ("time in tokyo", "2026-11-21 02:02 JST (UTC+9)"),
        ("days until 31 dec", "41 days"),
        ("today + 14 days", "2026-12-04"),
        ("1700000000", "2023-11-14T22:13:20Z"),
        ("2023-11-14T22:13:20Z", "1700000000"),
    ] {
        type_text(&mut l, query);
        let s = l.screen();
        assert_eq!(titles(&s)[0], expected, "query {query:?}");
        assert!(s.sections[0].rows[0].calculator, "query {query:?}");
        assert_eq!(
            l.handle(enter()),
            vec![Effect::CopyText(expected.into()), Effect::Hide],
            "enter on {query:?}"
        );
        l.show_root();
    }

    type_text(&mut l, "42");
    assert!(titles(&l.screen()).is_empty(), "bare 42 shows no calc row");
}
