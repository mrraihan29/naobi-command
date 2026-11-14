//! The Awake state machine and grammar, over a fake port and a clock moved by hand.
use naobi_awake::{parse, Awake, AwakeCommands, AwakePort, Cmd, Kind, Mode, View};
use naobi_core::command::{CommandSource, Cx, Query};
use naobi_core::ids::{ActionId, ItemId};
use std::sync::{Arc, Mutex};

const MIN: i64 = 60_000;
const T0: i64 = 1_700_000_000_000;

#[derive(Default)]
struct Port {
    /// `(on, display)` as last asked.
    power: Option<(bool, bool)>,
    timers: Option<Option<u64>>,
    tray: Option<View>,
    calls: usize,
}

impl AwakePort for Port {
    fn power(&mut self, on: bool, display: bool) {
        self.power = Some((on, display));
        self.calls += 1;
    }
    fn timers(&mut self, expiry_ms: Option<u64>) {
        self.timers = Some(expiry_ms);
    }
    fn tray(&mut self, view: View) {
        self.tray = Some(view);
    }
}

// @R-019
#[test]
fn r019_timed_mode_sets_and_clears_power_request() {
    let (mut a, mut p) = (Awake::default(), Port::default());
    a.timed(60 * MIN, &mut p, T0);
    assert_eq!(
        p.power,
        Some((true, false)),
        "request held, display free to sleep"
    );
    assert_eq!(p.timers, Some(Some(60 * MIN as u64)), "expiry armed");
    // A refresh a minute later keeps it and re-arms with the time that is left.
    a.on_timer(&mut p, T0 + MIN);
    assert_eq!(p.power, Some((true, false)));
    assert_eq!(p.timers, Some(Some(59 * MIN as u64)));
    // After expiry the request is released and the timers stop.
    a.on_timer(&mut p, T0 + 60 * MIN);
    assert_eq!(a.mode(), Mode::Off);
    assert_eq!(p.power, Some((false, false)));
    assert_eq!(p.timers, Some(None));
}

// @R-019
#[test]
fn r019_indefinite_has_no_timers_and_off_clears_everything() {
    let (mut a, mut p) = (Awake::default(), Port::default());
    assert!(!a.keeps_display());
    a.indefinite(&mut p, T0);
    assert_eq!((p.power, p.timers), (Some((true, false)), Some(None)));
    a.set_display(true, &mut p, T0);
    assert_eq!(p.power, Some((true, true)), "display on joins the request");
    a.off(&mut p, T0);
    assert_eq!((p.power, p.timers), (Some((false, false)), Some(None)));
    // The display choice is kept for the next time, but nothing is held while Off.
    assert!(a.keeps_display());
}

// @R-019
#[test]
fn r019_tray_icon_reflects_mode() {
    let (mut a, mut p) = (Awake::default(), Port::default());
    a.on_timer(&mut p, T0);
    assert_eq!(p.tray.as_ref().map(|v| v.kind), Some(Kind::Off));
    a.indefinite(&mut p, T0);
    assert_eq!(p.tray.as_ref().map(|v| v.kind), Some(Kind::Indefinite));
    a.timed(65 * MIN, &mut p, T0);
    let v = p.tray.clone().unwrap_or_else(|| panic!("no tray"));
    assert_eq!(v.kind, Kind::Timed);
    assert_eq!(v.tooltip, "Naobi Command Awake: 1 h 5 min left");
    // The tooltip counts down and rounds up to the minute.
    a.on_timer(&mut p, T0 + 5 * MIN + 1);
    assert_eq!(
        p.tray.map(|v| v.tooltip),
        Some("Naobi Command Awake: 1 h left".into())
    );
}

// @R-019
#[test]
fn r019_resume_reapplies_state() {
    let (mut a, mut p) = (Awake::default(), Port::default());
    a.timed(60 * MIN, &mut p, T0);
    let before = p.calls;
    a.on_resume(&mut p, T0 + 10 * MIN);
    assert!(p.calls > before, "the request is made again");
    assert_eq!(p.power, Some((true, false)));
    assert_eq!(p.timers, Some(Some(50 * MIN as u64)));
    // Slept past the end: Off is applied on resume.
    a.on_resume(&mut p, T0 + 90 * MIN);
    assert_eq!(a.mode(), Mode::Off);
    assert_eq!(p.power, Some((false, false)));
    // Indefinite comes back as it was.
    a.indefinite(&mut p, T0);
    p.power = None;
    a.on_resume(&mut p, T0 + 1000 * MIN);
    assert_eq!(p.power, Some((true, false)));
}

// @R-019
#[test]
fn r019_remaining_is_none_unless_timed() {
    let (mut a, mut p) = (Awake::default(), Port::default());
    assert_eq!(a.remaining(T0), None);
    a.timed(10 * MIN, &mut p, T0);
    assert_eq!(a.remaining(T0 + 4 * MIN), Some(6 * MIN));
    assert_eq!(a.remaining(T0 + 20 * MIN), Some(0));
}

// @D-024
#[test]
fn d024_awake_grammar_parses() {
    assert_eq!(parse("awake 1h"), Some(Cmd::For(60 * MIN)));
    assert_eq!(parse("awake 45m"), Some(Cmd::For(45 * MIN)));
    assert_eq!(parse("Awake 2H"), Some(Cmd::For(120 * MIN)));
    assert_eq!(parse("  awake   on "), Some(Cmd::On));
    assert_eq!(parse("awake off"), Some(Cmd::Off));
    assert_eq!(parse("awake display on"), Some(Cmd::Display(true)));
    assert_eq!(parse("awake display off"), Some(Cmd::Display(false)));
    assert_eq!(parse("awake 24h"), Some(Cmd::For(24 * 60 * MIN)));
    // Not commands.
    for bad in [
        "",
        "awake",
        "awake 0m",
        "awake 25h",
        "awake 9999999999999999999h",
        "awake -5m",
        "awake 1.5h",
        "awake 1d",
        "awake é",
        "awake 5é",
        "awake 日本",
        "awake 1441m",
        "awake h",
        "awake m",
        "awake 1h now",
        "awake display",
        "awake display maybe",
        "awake on off",
        "sleep 1h",
        "wake 1h",
        "awake on 1h",
    ] {
        assert_eq!(parse(bad), None, "{bad:?}");
    }
}

// @D-024
#[test]
fn d024_the_list_offers_30_min_1_h_2_h_and_a_typed_custom_one() {
    let ran = Arc::new(Mutex::new(Vec::<Cmd>::new()));
    let sink = ran.clone();
    let source = AwakeCommands::new(Arc::new(move |c| {
        sink.lock().unwrap_or_else(|e| e.into_inner()).push(c)
    }));
    let titles = |q: &str| -> Vec<String> {
        source
            .items(&Query { text: q.into() })
            .iter()
            .map(|i| i.title.clone())
            .collect()
    };
    let list = titles("");
    for want in [
        "Keep Awake for 30 minutes",
        "Keep Awake for 1 hour",
        "Keep Awake for 2 hours",
        "Turn Awake Off",
    ] {
        assert!(list.iter().any(|t| t == want), "{want}: {list:?}");
    }
    // `awake 45m` adds one item and puts it first; a typed duration already in the list adds none;
    // invalid input adds nothing.
    assert_eq!(titles("awake 45m")[0], "Keep Awake for 45 minutes");
    assert_eq!(titles("awake 45m").len(), list.len() + 1);
    assert_eq!(titles("awake 1h").len(), list.len());
    assert_eq!(titles("awake nonsense").len(), list.len());
    // Running an item hands its command to the app.
    let items = source.items(&Query {
        text: "awake 45m".into(),
    });
    let out = source.run(&items[0].id, ActionId("run".into()), &mut Cx::default());
    assert_eq!(
        out,
        naobi_core::command::Outcome::Effects(vec![naobi_core::effect::Effect::Hide])
    );
    assert_eq!(source.actions(&items[0].id).len(), 1);
    let id = ItemId("awake.display.on".into());
    source.run(&id, ActionId("run".into()), &mut Cx::default());
    assert_eq!(
        *ran.lock().unwrap_or_else(|e| e.into_inner()),
        vec![Cmd::For(45 * MIN), Cmd::Display(true)]
    );
}
