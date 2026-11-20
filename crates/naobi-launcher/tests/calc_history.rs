//! Calculator polish through the public engine: `ans` across restarts, 100-entry encrypted
//! history, CSS `px`/`rem` live config, and the Calculation History view.
#![allow(clippy::unwrap_used)] // naobi-allow: helpers outside #[test] fns
use naobi_core::cipher::TaggedCipher;
use naobi_core::command::{CommandSource, Query};
use naobi_core::db::Db;
use naobi_core::effect::Effect;
use naobi_core::ids::ViewId;
use naobi_core::ui::{Key, KeyChord, PreviewView, Screen, UiEvent};
use naobi_launcher::calc_history::{CalcHistory, KEEP};
use naobi_launcher::frecency::Frecency;
use naobi_launcher::providers::calc::{
    CalcConfig, CalcHistoryCommands, CalcHistoryView, CalcSource, HISTORY_CMD_ID, ROW_PREFIX,
    VIEW_ID,
};
use naobi_launcher::{Clock, Launcher};
use std::path::PathBuf;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::{Arc, Mutex};

const T0: i64 = 1_000_000_000_000;

struct FakeClock(Arc<AtomicI64>);
impl Clock for FakeClock {
    fn now_ms(&self) -> i64 {
        self.0.load(Ordering::SeqCst)
    }
}

struct TempDb(PathBuf);
impl TempDb {
    fn new(name: &str) -> Self {
        let path =
            std::env::temp_dir().join(format!("naobi-calc-test-{name}-{}.db", std::process::id()));
        let _ = std::fs::remove_file(&path);
        Self(path)
    }
}
impl Drop for TempDb {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

fn build_launcher(
    db_path: Option<&std::path::Path>,
    clock: Arc<AtomicI64>,
    config: CalcConfig,
) -> Launcher {
    let db = match db_path {
        Some(p) => Db::open(p, || [7; 16]).unwrap(),
        None => Db::open_in_memory().unwrap(),
    };
    let frecency = Frecency::open(
        match db_path {
            Some(p) => Db::open(p, || [7; 16]).unwrap(),
            None => Db::open_in_memory().unwrap(),
        },
        T0 / 1000,
    )
    .unwrap();
    let shared = Arc::new(Mutex::new(CalcHistory::open(db, Box::new(TaggedCipher))));
    let c1 = clock.clone();
    let c2 = clock.clone();
    Launcher::new(
        vec![
            Box::new(CalcSource::new(
                Some(shared.clone()),
                config,
                Box::new(move || c1.load(Ordering::SeqCst)),
            )),
            Box::new(CalcHistoryCommands),
        ],
        vec![Box::new(CalcHistoryView::new(
            shared,
            Box::new(move || c2.load(Ordering::SeqCst)),
        ))],
        frecency,
        Box::new(FakeClock(clock)),
    )
}

fn type_text(l: &mut Launcher, text: &str) {
    l.handle(UiEvent::TextChanged(text.into()));
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
        .flat_map(|sec| sec.rows.iter().map(|r| r.title.clone()))
        .collect()
}

#[test]
fn ans_chains_and_survives_restart_via_data_store() {
    let tmp = TempDb::new("ans-restart");
    let clock = Arc::new(AtomicI64::new(T0));
    {
        let mut l = build_launcher(Some(&tmp.0), clock.clone(), CalcConfig::default());
        type_text(&mut l, "$120 + $45");
        assert_eq!(titles(&l.screen()), vec!["165"]);
        assert_eq!(
            press(&mut l, Key::Enter, false),
            vec![Effect::CopyText("165".into()), Effect::Hide]
        );
        clock.fetch_add(1_000, Ordering::SeqCst);

        // Immediate chaining in the same session.
        l.show_root();
        type_text(&mut l, "ans * 2");
        assert_eq!(titles(&l.screen()), vec!["330"]);
        assert_eq!(
            press(&mut l, Key::Enter, false),
            vec![Effect::CopyText("330".into()), Effect::Hide]
        );
    }

    // Simulated restart: fresh Launcher + CalcHistory opened from the same naobi.db.
    clock.fetch_add(1_000, Ordering::SeqCst);
    let mut l2 = build_launcher(Some(&tmp.0), clock, CalcConfig::default());
    type_text(&mut l2, "ans + 70");
    assert_eq!(titles(&l2.screen()), vec!["400"]);
}

#[test]
fn calc_history_retains_at_most_100_encrypted_entries() {
    let tmp = TempDb::new("retention");
    let db = Db::open(&tmp.0, || [3; 16]).unwrap();
    let mut h = CalcHistory::open(db, Box::new(TaggedCipher));
    for i in 0..=KEEP {
        h.record(&format!("{i} + 1"), &format!("{}", i + 1), T0 + i * 1000)
            .unwrap();
    }
    let entries = h.list();
    assert_eq!(entries.len() as i64, KEEP);
    assert_eq!(entries[0].expr.as_str(), &format!("{KEEP} + 1"));
    assert_eq!(entries.last().unwrap().expr.as_str(), "1 + 1");
    drop(h);

    // Raw table holds cipher blobs only, never plaintext.
    let conn = rusqlite::Connection::open(&tmp.0).unwrap();
    let (expr_blob, res_blob): (Vec<u8>, Vec<u8>) = conn
        .query_row(
            "SELECT expr_enc, result_enc FROM calc_history ORDER BY calculated_at DESC, id DESC LIMIT 1",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!(expr_blob, format!("ENC1:{KEEP} + 1").as_bytes());
    assert_eq!(res_blob, format!("ENC1:{}", KEEP + 1).as_bytes());
}

#[test]
fn calculation_history_view_searches_previews_copies_and_deletes() {
    let tmp = TempDb::new("view");
    let clock = Arc::new(AtomicI64::new(T0));
    let mut l = build_launcher(Some(&tmp.0), clock.clone(), CalcConfig::default());

    for (expr, step) in [("2 + 3", 1_000), ("Rp50000 * 2", 61_000)] {
        clock.store(T0 + step, Ordering::SeqCst);
        l.show_root();
        type_text(&mut l, expr);
        press(&mut l, Key::Enter, false);
    }

    // Open Calculation History from root search.
    l.show_root();
    type_text(&mut l, "Calculation History");
    assert_eq!(
        press(&mut l, Key::Enter, false),
        vec![Effect::PushView(ViewId(VIEW_ID.into()))]
    );
    let s = l.screen();
    assert_eq!(s.view.name, "Calculation history");
    assert!(s.view.back);
    assert_eq!(titles(&s), vec!["100000", "5"]);

    // Preview and privacy-safe Fact + icon_key.
    assert_eq!(
        s.preview,
        Some(PreviewView::Text("Rp50000 * 2 = 100000".into()))
    );
    assert_eq!(s.facts.len(), 1);
    assert_eq!(s.facts[0].label, "Calculated");
    assert_eq!(s.facts[0].value, "just now");
    assert_eq!(s.sections[0].rows[0].icon_key.as_deref(), Some("g:calc"));

    // Search filters by expression or result.
    type_text(&mut l, "Rp50000");
    assert_eq!(titles(&l.screen()), vec!["100000"]);
    type_text(&mut l, "");
    assert_eq!(titles(&l.screen()), vec!["100000", "5"]);

    // Ctrl+Enter copies expression; Enter copies result.
    assert_eq!(
        press(&mut l, Key::Enter, true),
        vec![Effect::CopyText("Rp50000 * 2".into()), Effect::Hide]
    );
    assert_eq!(
        press(&mut l, Key::Enter, false),
        vec![Effect::CopyText("100000".into()), Effect::Hide]
    );

    // Ctrl+X deletes the selected entry and stays in the view.
    let fx = press(&mut l, Key::X, true);
    assert!(!fx.contains(&Effect::Hide));
    assert_eq!(titles(&l.screen()), vec!["5"]);

    // Privacy check on ItemId: digits only after ROW_PREFIX.
    let view = CalcHistoryView::new(
        Arc::new(Mutex::new(CalcHistory::open(
            Db::open(&tmp.0, || [7; 16]).unwrap(),
            Box::new(TaggedCipher),
        ))),
        Box::new(|| T0),
    );
    for it in view.items(&Query::default()).iter() {
        let suffix = it.id.0.strip_prefix(ROW_PREFIX).unwrap();
        assert!(!suffix.is_empty() && suffix.bytes().all(|b| b.is_ascii_digit()));
    }
    assert_eq!(
        CalcHistoryCommands.items(&Query::default())[0].id.0,
        HISTORY_CMD_ID
    );
}

#[test]
fn css_rem_base_px_config_updates_live_and_calc_skips_frecency() {
    let tmp = TempDb::new("css-frec");
    let clock = Arc::new(AtomicI64::new(T0));
    let cfg = CalcConfig::default();
    let mut l = build_launcher(Some(&tmp.0), clock, cfg.clone());

    type_text(&mut l, "32 px to rem");
    assert_eq!(titles(&l.screen()), vec!["2 rem"]);
    press(&mut l, Key::Enter, false);

    cfg.set_rem_base_px(10.0);
    l.show_root();
    type_text(&mut l, "25 px to rem");
    assert_eq!(titles(&l.screen()), vec!["2.5 rem"]);
    drop(l);

    // Calculator rows never pollute frecency.
    let conn = rusqlite::Connection::open(&tmp.0).unwrap();
    let count: i64 = conn
        .query_row("SELECT COUNT(*) FROM frecency", [], |r| r.get(0))
        .unwrap();
    assert_eq!(count, 0);
}

#[test]
fn currency_conversion_in_launcher_formats_copies_raw_and_offers_action_panel_copy_with_currency() {
    let tmp = TempDb::new("fx-conv");
    let fx_path = std::env::temp_dir().join(format!("naobi-fx-conv-{}.json", std::process::id()));
    let _ = std::fs::remove_file(&fx_path);
    std::fs::write(
        &fx_path,
        r#"{"amount":1.0,"base":"EUR","date":"2026-11-19","rates":{"USD":1.05,"IDR":19065.9,"JPY":162.75}}"#,
    )
    .unwrap();

    let clock = Arc::new(AtomicI64::new(T0));
    let cfg = CalcConfig::default();
    cfg.set_fx_path(Some(fx_path.clone()));
    let mut l = build_launcher(Some(&tmp.0), clock, cfg.clone());

    type_text(&mut l, "53 usd");
    let s = l.screen();
    assert_eq!(titles(&s), vec!["962,374.00 IDR"]);
    assert_eq!(
        s.sections[0].rows[0].subtitle,
        "53 USD to IDR · Rates 2026-11-19"
    );

    // Primary action (Enter) copies raw numeric value.
    assert_eq!(
        press(&mut l, Key::Enter, false),
        vec![Effect::CopyText("962374.00".into()), Effect::Hide]
    );

    // `ans` chains from the copied numeric currency value.
    l.show_root();
    type_text(&mut l, "ans + 26");
    assert_eq!(titles(&l.screen()), vec!["962400"]);

    // Action panel offers "Copy with currency".
    l.show_root();
    type_text(&mut l, "53 usd");
    press(&mut l, Key::K, true);
    let panel = l.screen().action_panel.expect("action panel open");
    let labels: Vec<&str> = panel.actions.iter().map(|a| a.label.as_str()).collect();
    assert_eq!(labels, vec!["Copy", "Copy with currency"]);
    press(&mut l, Key::Down, false);
    assert_eq!(
        press(&mut l, Key::Enter, false),
        vec![Effect::CopyText("962,374.00 IDR".into()), Effect::Hide]
    );

    // Live config switch to EUR + `id` number format.
    cfg.set_currency("EUR", "id");
    l.show_root();
    type_text(&mut l, "53 usd");
    assert_eq!(titles(&l.screen()), vec!["50,48 EUR"]);

    l.show_root();
    type_text(&mut l, "53 usd to idr");
    assert_eq!(titles(&l.screen()), vec!["962.374,00 IDR"]);
    assert_eq!(
        press(&mut l, Key::Enter, false),
        vec![Effect::CopyText("962374.00".into()), Effect::Hide]
    );

    l.show_root();
    type_text(&mut l, "53k usd in jpy");
    assert_eq!(titles(&l.screen()), vec!["8.215.000,00 JPY"]);

    let _ = std::fs::remove_file(&fx_path);
}

#[test]
fn currency_missing_cache_shows_helper_row_and_notice_without_failing_silently() {
    let tmp = TempDb::new("fx-missing");
    let fx_path =
        std::env::temp_dir().join(format!("naobi-fx-missing-{}.json", std::process::id()));
    let _ = std::fs::remove_file(&fx_path);

    let clock = Arc::new(AtomicI64::new(T0));
    let cfg = CalcConfig::default();
    cfg.set_fx_path(Some(fx_path.clone()));
    let mut l = build_launcher(Some(&tmp.0), clock, cfg);

    type_text(&mut l, "53 usd");
    let s = l.screen();
    assert_eq!(titles(&s), vec!["No cached currency rates"]);
    assert!(
        s.sections[0].rows[0]
            .subtitle
            .to_ascii_lowercase()
            .contains("refresh"),
        "{}",
        s.sections[0].rows[0].subtitle
    );

    // Pressing Enter shows a Notice and keeps Launcher open.
    let fx = press(&mut l, Key::Enter, false);
    assert!(!fx.contains(&Effect::Hide));
    assert!(matches!(fx.as_slice(), [Effect::Notice(_)]), "{fx:?}");
    assert!(l.screen().notice.is_some());

    // Dropping fx-rates.json locally enables conversion immediately without restart or network.
    std::fs::write(
        &fx_path,
        r#"{"base":"EUR","date":"2026-11-19","rates":{"USD":1.05,"IDR":19065.9}}"#,
    )
    .unwrap();
    l.show_root();
    type_text(&mut l, "53 usd");
    assert_eq!(titles(&l.screen()), vec!["962,374.00 IDR"]);

    let _ = std::fs::remove_file(&fx_path);
}
