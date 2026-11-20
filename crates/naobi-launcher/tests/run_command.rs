//! The `>` run command through the public engine: claim path, run history, launch targets.
#![allow(clippy::unwrap_used)] // naobi-allow: helpers outside #[test] fns; clippy.toml only exempts test fns themselves
use naobi_core::cipher::{Cipher, CipherError, TaggedCipher};
use naobi_core::command::{CommandSource, Cx, Outcome, Query};
use naobi_core::db::Db;
use naobi_core::effect::{Effect, LaunchTarget};
use naobi_core::ids::{ActionId, ItemId};
use naobi_core::item::{Action, Item, ItemKind};
use naobi_core::notice::NoticeKind;
use naobi_core::ui::{Key, KeyChord, Screen, UiEvent};
use naobi_launcher::frecency::Frecency;
use naobi_launcher::providers::calc::CalcSource;
use naobi_launcher::providers::run::{
    split_first, Program, ProgramLookup, RunSource, HISTORY_PREFIX, TYPED_ID,
};
use naobi_launcher::run_history::{RunHistory, KEEP, MAX_AGE_MS};
use naobi_launcher::{Clock, Launcher};
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use zeroize::Zeroizing;

const T0: i64 = 1_000_000_000_000;
const SECOND: i64 = 1000;

struct Clk;
impl Clock for Clk {
    fn now_ms(&self) -> i64 {
        T0
    }
}

struct Lookup(Vec<(&'static str, Program)>);
impl ProgramLookup for Lookup {
    fn find(&self, token: &str) -> Option<Program> {
        self.0
            .iter()
            .find(|(t, _)| *t == token)
            .map(|(_, p)| p.clone())
    }
}

fn lookup() -> Box<dyn ProgramLookup> {
    let p = |path: &str, gui| Program {
        path: path.into(),
        gui,
    };
    Box::new(Lookup(vec![
        ("notepad", p("C:\\Windows\\notepad.exe", true)),
        ("C:\\a b\\x.exe", p("C:\\a b\\x.exe", true)),
        ("ping", p("C:\\Windows\\ping.exe", false)),
    ]))
}

/// A source with an item titled ">ping", to show a claimed query never lists it.
struct Decoy;
impl CommandSource for Decoy {
    fn items(&self, _q: &Query) -> Arc<[Item]> {
        vec![Item {
            id: ItemId("decoy".into()),
            title: ">ping".into(),
            subtitle: String::new(),
            keywords: vec![],
            alias: None,
            kind: ItemKind::Command,
            primary: ActionId("open".into()),
            secondary: None,
        }]
        .into()
    }
    fn actions(&self, _id: &ItemId) -> Vec<Action> {
        vec![]
    }
    fn run(&self, _id: &ItemId, _a: ActionId, _cx: &mut Cx) -> Outcome {
        Outcome::Done
    }
}

fn history(lines: &[&str]) -> RunHistory {
    let mut h =
        RunHistory::open(Db::open_in_memory().unwrap(), Box::new(TaggedCipher), T0).unwrap();
    for (i, l) in lines.iter().enumerate() {
        h.record(l, None, T0 + i as i64 * SECOND).unwrap();
    }
    h
}

fn launcher(history: Option<RunHistory>, frecency: Frecency) -> Launcher {
    let run = RunSource::new(history, lookup(), Box::new(|| T0 + 100 * SECOND));
    Launcher::new(
        vec![
            Box::new(Decoy),
            Box::new(CalcSource::default()),
            Box::new(run),
        ],
        vec![],
        frecency,
        Box::new(Clk),
    )
}

fn mem_frecency() -> Frecency {
    Frecency::open(Db::open_in_memory().unwrap(), T0 / 1000).unwrap()
}

fn type_text(l: &mut Launcher, t: &str) {
    l.handle(UiEvent::TextChanged(t.into()));
}

fn press(l: &mut Launcher, key: Key, ctrl: bool) -> Vec<Effect> {
    l.handle(UiEvent::Key(KeyChord {
        key,
        ctrl,
        alt: false,
        shift: false,
    }))
}

fn titles(s: &Screen) -> Vec<String> {
    s.sections
        .iter()
        .flat_map(|x| x.rows.iter().map(|r| r.title.clone()))
        .collect()
}

fn launches(effects: &[Effect]) -> Vec<LaunchTarget> {
    effects
        .iter()
        .filter_map(|e| match e {
            Effect::Launch(t) => Some(t.clone()),
            _ => None,
        })
        .collect()
}

fn temp_db(name: &str) -> (PathBuf, Db) {
    let path = std::env::temp_dir().join(format!("naobi-{name}-{}.db", std::process::id()));
    let _ = std::fs::remove_file(&path);
    let db = Db::open(&path, || [3; 16]).unwrap();
    (path, db)
}

// @R-024
#[test]
fn r024_prefix_lists_only_run_rows_and_no_file_search() {
    let mut l = launcher(Some(history(&["ping a", "dir", "pwd"])), mem_frecency());
    type_text(&mut l, ">");
    let s = l.screen();
    assert_eq!(
        titles(&s),
        ["pwd", "dir", "ping a"],
        "newest first, no decoy"
    );
    assert!(l.file_query().is_none());
    assert!(s.empty.is_none());

    type_text(&mut l, ">pi");
    let s = l.screen();
    assert_eq!(
        titles(&s),
        ["pi", "ping a"],
        "typed row, then matching history"
    );
    assert_eq!(s.sections.len(), 1);
    assert!(l.file_query().is_none());
    assert!(!l.file_results(l.generation(), 0, vec![]));

    type_text(&mut l, "ping");
    assert!(
        l.file_query().is_some(),
        "an ordinary query still searches files"
    );

    let mut empty = launcher(Some(history(&[])), mem_frecency());
    type_text(&mut empty, ">");
    let e = empty.screen().empty.unwrap();
    assert_eq!(e.title, "No command lines run yet");
    assert_eq!(e.helper, "Type a command line after > and press Enter");
}

// @R-024
#[test]
fn r024_split_first_cases() {
    assert_eq!(split_first("notepad a.txt"), ("notepad", "a.txt"));
    assert_eq!(
        split_first("\"C:\\a b\\x.exe\" -y"),
        ("C:\\a b\\x.exe", "-y")
    );
    assert_eq!(split_first("\"C:\\a b\\x.exe"), ("C:\\a b\\x.exe", ""));
    assert_eq!(split_first("ping"), ("ping", ""));
    assert_eq!(split_first("a   b  c"), ("a", "b  c"));
}

// @R-024
#[test]
fn r024_gui_exe_starts_directly_else_console() {
    let cases = [
        (
            ">notepad a.txt",
            LaunchTarget::Program {
                path: "C:\\Windows\\notepad.exe".into(),
                args: "a.txt".into(),
            },
        ),
        (
            ">\"C:\\a b\\x.exe\" -y",
            LaunchTarget::Program {
                path: "C:\\a b\\x.exe".into(),
                args: "-y".into(),
            },
        ),
        (
            ">ping google.com -t",
            LaunchTarget::Console("ping google.com -t".into()),
        ),
        (">unknown x", LaunchTarget::Console("unknown x".into())),
    ];
    for (q, want) in cases {
        let mut l = launcher(Some(history(&[])), mem_frecency());
        type_text(&mut l, q);
        let fx = press(&mut l, Key::Enter, false);
        assert_eq!(launches(&fx), [want], "{q}");
        assert!(fx.contains(&Effect::Hide));
    }
}

// @R-024
#[test]
fn r024_rerun_moves_to_top_without_duplicate() {
    let mut l = launcher(Some(history(&["one", "two", "three"])), mem_frecency());
    type_text(&mut l, ">");
    press(&mut l, Key::Down, false);
    press(&mut l, Key::Down, false); // "one"
    let fx = press(&mut l, Key::Enter, false);
    assert_eq!(launches(&fx), [LaunchTarget::Console("one".into())]);
    l.show_root();
    type_text(&mut l, ">");
    assert_eq!(titles(&l.screen()), ["one", "three", "two"]);
}

// @R-024
#[test]
fn r024_delete_removes_one_from_run_history() {
    let mut l = launcher(Some(history(&["one", "two", "three"])), mem_frecency());
    type_text(&mut l, ">");
    press(&mut l, Key::Down, false); // "two"
    press(&mut l, Key::K, true);
    let panel = l.screen().action_panel.unwrap();
    let labels: Vec<_> = panel.actions.iter().map(|a| a.label.as_str()).collect();
    assert_eq!(labels, ["Run", "Delete from Run History"]);
    press(&mut l, Key::Down, false);
    let fx = press(&mut l, Key::Enter, false);
    assert!(!fx.contains(&Effect::Hide), "the Launcher stays open");
    assert_eq!(titles(&l.screen()), ["three", "one"]);
}

// @R-024
#[test]
fn r024_history_retention_100_and_30_days() {
    let mut h = history(&[]);
    for i in 0..=KEEP {
        h.record(&format!("cmd {i}"), None, T0 + i * SECOND)
            .unwrap();
    }
    let lines: Vec<String> = h.lines().iter().map(|l| l.line.to_string()).collect();
    assert_eq!(lines.len() as i64, KEEP);
    assert_eq!(lines[0], format!("cmd {KEEP}"));
    assert_eq!(lines.last().unwrap(), "cmd 1", "the oldest one went");

    let mut h = history(&[]);
    h.record("old", None, T0).unwrap();
    h.record("edge", None, T0 + 1).unwrap();
    // "old" is 30 days + 1 ms old, "edge" exactly 30 days.
    h.record("new", None, T0 + MAX_AGE_MS + 1).unwrap();
    let lines: Vec<String> = h.lines().iter().map(|l| l.line.to_string()).collect();
    assert_eq!(lines, ["new", "edge"]);

    // open purges too, with nothing decrypted.
    let (path, db) = temp_db("retention");
    let mut h = RunHistory::open(db, Box::new(TaggedCipher), T0).unwrap();
    h.record("stale", None, T0).unwrap();
    drop(h);
    let db = Db::open(&path, || [3; 16]).unwrap();
    let mut h = RunHistory::open(db, Box::new(TaggedCipher), T0 + MAX_AGE_MS + 1).unwrap();
    assert!(h.lines().is_empty());
    drop(h);
    let _ = std::fs::remove_file(&path);
}

// @R-024
#[test]
fn r024_history_stores_only_cipher_blobs() {
    let (path, db) = temp_db("blobs");
    let mut h = RunHistory::open(db, Box::new(TaggedCipher), T0).unwrap();
    h.record("secret-token --flag", Some("C:\\x\\prog.exe"), T0)
        .unwrap();
    let conn = rusqlite::Connection::open(&path).unwrap();
    conn.execute(
        "INSERT INTO run_history (line_enc, program_enc, last_run_at) VALUES (?1, NULL, ?2)",
        rusqlite::params![b"garbage".to_vec(), T0 + 1],
    )
    .unwrap();
    let rows: Vec<(Vec<u8>, Vec<u8>)> = conn
        .prepare("SELECT line_enc, program_enc FROM run_history WHERE program_enc IS NOT NULL")
        .unwrap()
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap()
        .flatten()
        .collect();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].0, b"ENC1:secret-token --flag");
    assert_eq!(rows[0].1, b"ENC1:C:\\x\\prog.exe");
    h.forget();
    let lines = h.lines();
    assert_eq!(lines.len(), 1, "the blob that does not decrypt is skipped");
    assert_eq!(lines[0].program.as_deref(), Some("C:\\x\\prog.exe"));
    drop(h);
    drop(conn);
    let _ = std::fs::remove_file(&path);
}

// @R-024
#[test]
fn r024_run_rows_not_in_frecency_and_ids_hold_no_text() {
    let (path, fdb) = temp_db("frec");
    let frecency = Frecency::open(fdb, T0 / 1000).unwrap();
    let mut l = launcher(Some(history(&["hist secret"])), frecency);

    type_text(&mut l, ">secret");
    let s = l.screen();
    let keys: Vec<_> = s.sections[0]
        .rows
        .iter()
        .map(|r| r.icon_key.as_deref())
        .collect();
    assert_eq!(keys, [Some("g:run"), Some("g:run.history")]);
    // Both rows ran (Enter on each), neither is counted.
    press(&mut l, Key::Enter, false);
    l.show_root();
    type_text(&mut l, ">");
    press(&mut l, Key::Enter, false);
    drop(l);

    let conn = rusqlite::Connection::open(&path).unwrap();
    let n: i64 = conn
        .query_row("SELECT COUNT(*) FROM frecency", [], |r| r.get(0))
        .unwrap();
    assert_eq!(n, 0);
    drop(conn);
    let _ = std::fs::remove_file(&path);

    // Ids: the typed id, or the history prefix plus digits.
    let run = RunSource::new(Some(history(&["a", "b"])), lookup(), Box::new(|| T0));
    for it in run.items(&Query { text: ">c".into() }).iter() {
        let ok = it.id.0 == TYPED_ID
            || it
                .id
                .0
                .strip_prefix(HISTORY_PREFIX)
                .is_some_and(|d| !d.is_empty() && d.bytes().all(|b| b.is_ascii_digit()));
        assert!(ok, "{:?}", it.id);
    }
}

// @R-024
#[test]
fn r024_first_enter_runs_and_no_admin_action() {
    let mut l = launcher(Some(history(&["dir"])), mem_frecency());
    type_text(&mut l, ">dir");
    let fx = press(&mut l, Key::Enter, false);
    assert_eq!(launches(&fx).len(), 1, "no arming");
    type_text(&mut l, ">x");
    press(&mut l, Key::K, true);
    let panel = l.screen().action_panel.unwrap();
    assert!(panel
        .actions
        .iter()
        .all(|a| !a.label.to_lowercase().contains("admin")));
}

// @R-024
#[test]
fn r024_too_long_line_is_refused_and_launcher_stays() {
    let mut l = launcher(Some(history(&[])), mem_frecency());
    type_text(&mut l, &format!(">{}", "x".repeat(8001)));
    let fx = press(&mut l, Key::Enter, false);
    assert!(launches(&fx).is_empty());
    assert!(!fx.contains(&Effect::Hide));
    let Some(Effect::Notice(n)) = fx.first() else {
        panic!("a Notice was expected");
    };
    assert_eq!(n.subject, "Run");
    assert_eq!(n.message, "That command line is too long.");

    // 8000 is allowed.
    type_text(&mut l, &format!(">{}", "x".repeat(8000)));
    assert_eq!(launches(&press(&mut l, Key::Enter, false)).len(), 1);
}

// @R-024
#[test]
fn r024_unsaved_history_still_runs_with_a_warning() {
    let mut l = launcher(None, mem_frecency());
    type_text(&mut l, ">dir");
    let fx = press(&mut l, Key::Enter, false);
    assert_eq!(launches(&fx), [LaunchTarget::Console("dir".into())]);
    let warn = fx.iter().find_map(|e| match e {
        Effect::Notice(n) => Some(n),
        _ => None,
    });
    let n = warn.unwrap();
    assert_eq!(n.subject, "Run history");
    assert_eq!(n.message, "This command line was not saved.");
    assert_eq!(n.kind, NoticeKind::Warning);
}

struct Counting(Arc<AtomicUsize>);
impl Cipher for Counting {
    fn encrypt(&self, p: &[u8]) -> Result<Vec<u8>, CipherError> {
        TaggedCipher.encrypt(p)
    }
    fn decrypt(&self, b: &[u8]) -> Result<Zeroizing<Vec<u8>>, CipherError> {
        self.0.fetch_add(1, Ordering::SeqCst);
        TaggedCipher.decrypt(b)
    }
}

// @R-024
#[test]
fn r024_decrypted_lines_are_dropped_on_a_non_run_query() {
    let n = Arc::new(AtomicUsize::new(0));
    let mut h = RunHistory::open(
        Db::open_in_memory().unwrap(),
        Box::new(Counting(n.clone())),
        T0,
    )
    .unwrap();
    h.record("one", None, T0).unwrap();
    n.store(0, Ordering::SeqCst);
    let mut l = launcher(Some(h), mem_frecency());
    assert_eq!(n.load(Ordering::SeqCst), 0, "no `>` yet: nothing decrypted");
    type_text(&mut l, ">");
    type_text(&mut l, ">o");
    assert_eq!(n.load(Ordering::SeqCst), 1, "cached while `>` is shown");
    type_text(&mut l, "x");
    type_text(&mut l, ">");
    assert_eq!(n.load(Ordering::SeqCst), 2, "dropped by the plain query");
    l.show_root();
    type_text(&mut l, ">");
    assert_eq!(n.load(Ordering::SeqCst), 3, "dropped by the show");
}
