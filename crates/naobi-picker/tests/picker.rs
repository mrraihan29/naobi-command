//! Formats, the 20-pick history, the view and the snapshot's life.
use naobi_core::command::{CommandSource, Cx, Outcome, Query};
use naobi_core::db::Db;
use naobi_core::effect::Effect;
use naobi_core::ids::{ActionId, ItemId};
use naobi_core::item::ItemKind;
use naobi_core::view_source::{Preview, ViewSource};
use naobi_picker::format::{format, Format, Rgb};
use naobi_picker::{
    flip, pick, ColorHistory, ColorHistoryView, PickerCommands, ScreenCapture, VIEW_ID,
};
use std::cell::Cell;
use std::sync::{Arc, Mutex};

fn rgb(r: u8, g: u8, b: u8) -> Rgb {
    Rgb { r, g, b }
}

fn history() -> ColorHistory {
    ColorHistory::new(Db::open_in_memory().unwrap_or_else(|e| panic!("db: {e}")))
}

// @R-017
#[test]
fn r017_formats_hex_rgb_hsl_oklch() {
    let c = rgb(0x91, 0x4A, 0x3B);
    assert_eq!(format(c, Format::Hex), "#914A3B");
    assert_eq!(format(c, Format::Rgb), "rgb(145, 74, 59)");
    assert_eq!(format(c, Format::Hsl), "hsl(10, 42%, 40%)");
    // Reference values from the OKLab definition.
    assert_eq!(format(c, Format::Oklch), "oklch(49.3% 0.099 33.1)");
    // Black, white and a gray have no hue and no saturation.
    assert_eq!(format(rgb(0, 0, 0), Format::Hsl), "hsl(0, 0%, 0%)");
    assert_eq!(format(rgb(255, 255, 255), Format::Hsl), "hsl(0, 0%, 100%)");
    assert_eq!(
        format(rgb(255, 255, 255), Format::Oklch),
        "oklch(100.0% 0.000 0.0)"
    );
    assert_eq!(format(rgb(0, 0, 0), Format::Oklch), "oklch(0.0% 0.000 0.0)");
    // The primaries sit on the hue circle where HSL says they do.
    assert_eq!(format(rgb(255, 0, 0), Format::Hsl), "hsl(0, 100%, 50%)");
    assert_eq!(format(rgb(0, 255, 0), Format::Hsl), "hsl(120, 100%, 50%)");
    assert_eq!(format(rgb(0, 0, 255), Format::Hsl), "hsl(240, 100%, 50%)");
    assert_eq!(
        format(rgb(255, 0, 0), Format::Oklch),
        "oklch(62.8% 0.258 29.2)"
    );
    assert_eq!(Rgb::from_packed(c.packed()), c);
    assert_eq!(c.packed(), 0x914A3B);
    // Green and blue are the largest channel, away from the pure primaries; dark values use the
    // linear segment of the sRGB curve.
    for (c, hsl, oklch) in [
        (
            rgb(10, 200, 100),
            "hsl(148, 90%, 41%)",
            "oklch(72.9% 0.194 151.2)",
        ),
        (
            rgb(20, 40, 200),
            "hsl(233, 82%, 43%)",
            "oklch(41.0% 0.238 266.2)",
        ),
        (
            rgb(3, 10, 7),
            "hsl(154, 54%, 3%)",
            "oklch(13.4% 0.016 169.2)",
        ),
    ] {
        assert_eq!(format(c, Format::Hsl), hsl);
        assert_eq!(format(c, Format::Oklch), oklch);
    }
}

// @R-017
#[test]
fn r017_history_drops_oldest_at_21() {
    let mut h = history();
    for i in 0..21u8 {
        h.push(rgb(i, 0, 0), 1000 + i64::from(i))
            .unwrap_or_else(|e| panic!("push: {e}"));
    }
    let list = h.list().unwrap_or_default();
    assert_eq!(list.len(), 20);
    assert_eq!(list[0].rgb, rgb(20, 0, 0), "newest first");
    assert_eq!(list[19].rgb, rgb(1, 0, 0), "the first pick is gone");
    // A clock that went backwards does not lose the new pick.
    h.push(rgb(7, 7, 7), 0).unwrap_or_else(|e| panic!("{e}"));
    assert!(h
        .list()
        .unwrap_or_default()
        .iter()
        .any(|p| p.rgb == rgb(7, 7, 7)));
    // Two picks in the same millisecond keep their order.
    h.push(rgb(0, 9, 0), 5000).unwrap_or_else(|e| panic!("{e}"));
    h.push(rgb(0, 8, 0), 5000).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(h.list().unwrap_or_default()[0].rgb, rgb(0, 8, 0));
}

fn view() -> (ColorHistoryView, Arc<Mutex<ColorHistory>>) {
    let shared = Arc::new(Mutex::new(history()));
    (ColorHistoryView::new(shared.clone()), shared)
}

// @R-017
#[test]
fn r017_the_view_lists_swatch_rows_and_copies_every_format() {
    let (v, shared) = view();
    assert!(v.items(&Query::default()).is_empty(), "empty state");
    assert_eq!(v.empty_title(), "No colors picked yet");
    shared
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .push(rgb(0x91, 0x4A, 0x3B), 1)
        .unwrap_or_else(|e| panic!("{e}"));
    let items = v.items(&Query::default());
    assert_eq!(items.len(), 1);
    assert_eq!(
        (items[0].title.as_str(), items[0].kind),
        ("#914A3B", ItemKind::Color)
    );
    assert_eq!(items[0].subtitle, "rgb(145, 74, 59)");
    // Search takes the hex with or without the hash, in any case.
    for hit in ["914a", "#914A3B", "4a3"] {
        assert_eq!(v.items(&Query { text: hit.into() }).len(), 1, "{hit}");
    }
    assert!(v
        .items(&Query {
            text: "ffffff".into()
        })
        .is_empty());
    let labels: Vec<_> = v
        .actions(&items[0].id)
        .into_iter()
        .map(|a| a.label)
        .collect();
    assert_eq!(
        labels,
        ["Copy as HEX", "Copy as RGB", "Copy as HSL", "Copy as OKLCH"]
    );
    for (action, want) in [
        ("hex", "#914A3B"),
        ("rgb", "rgb(145, 74, 59)"),
        ("hsl", "hsl(10, 42%, 40%)"),
        ("oklch", "oklch(49.3% 0.099 33.1)"),
    ] {
        let out = v.run(&items[0].id, ActionId(action.into()), &mut Cx::default());
        assert_eq!(out, Outcome::Effects(vec![Effect::CopyText(want.into())]));
    }
    let gone = v.run(
        &ItemId("color:999".into()),
        ActionId("hex".into()),
        &mut Cx::default(),
    );
    assert_eq!(gone, Outcome::Done, "a pick that is gone copies nothing");
    assert!(
        !v.opened_by_hotkey() && !v.ranked(),
        "Esc pops; the view keeps its order"
    );
    assert_eq!(v.placeholder(), "Search colors...");
    assert!(matches!(v.preview(&items[0].id), Some(Preview::Text(t)) if t.lines().count() == 4));
}

// @R-017
#[test]
fn r017_the_commands_run_the_picker_and_open_the_history() {
    let items = PickerCommands.items(&Query::default());
    let titles: Vec<_> = items.iter().map(|i| i.title.as_str()).collect();
    assert_eq!(titles, ["Pick Color", "Color History"]);
    assert_eq!(PickerCommands.actions(&items[0].id).len(), 1);
    let run =
        |i: usize| PickerCommands.run(&items[i].id, ActionId("run".into()), &mut Cx::default());
    assert!(matches!(
        run(0),
        Outcome::Effects(e) if matches!(&e[..], [Effect::RunHotkey(h)] if h.0 == "color_picker")
    ));
    assert!(matches!(
        run(1),
        Outcome::Effects(e) if matches!(&e[..], [Effect::PushView(v)] if v.0 == VIEW_ID)
    ));
}

struct Frame<'a>(&'a Cell<i32>);

impl Drop for Frame<'_> {
    fn drop(&mut self) {
        self.0.set(self.0.get() - 1);
    }
}

struct Grab<'a>(&'a Cell<i32>);

impl<'a> ScreenCapture for Grab<'a> {
    type Frame = Frame<'a>;
    fn capture(&mut self) -> Option<Frame<'a>> {
        self.0.set(self.0.get() + 1);
        Some(Frame(self.0))
    }
}

// @D-023
#[test]
fn d023_snapshot_dropped_after_pick() {
    let live = Cell::new(0);
    let seen = Cell::new(0);
    // A pick: the overlay sees one live snapshot, and none is left afterwards.
    let got = pick(&mut Grab(&live), |_| {
        seen.set(live.get());
        Some(rgb(1, 2, 3))
    });
    assert_eq!((got, seen.get(), live.get()), (Some(rgb(1, 2, 3)), 1, 0));
    // Esc or a right click: no color, no snapshot left.
    assert_eq!(pick(&mut Grab(&live), |_| None), None);
    assert_eq!(live.get(), 0);
    // No capture, no overlay.
    struct Blind;
    impl ScreenCapture for Blind {
        type Frame = ();
        fn capture(&mut self) -> Option<()> {
            None
        }
    }
    assert_eq!(pick(&mut Blind, |()| Some(rgb(9, 9, 9))), None);
}

// @R-017
#[test]
fn r017_the_readout_flips_near_a_screen_edge() {
    assert_eq!(
        flip(100, 80, 1000, 16),
        116,
        "room on the right: after the cursor"
    );
    assert_eq!(flip(904, 80, 1000, 16), 920, "fits exactly");
    assert_eq!(
        flip(905, 80, 1000, 16),
        809,
        "one more pixel: before the cursor"
    );
    assert_eq!(
        flip(10, 80, 50, 16),
        0,
        "a screen too small keeps it on screen"
    );
}
