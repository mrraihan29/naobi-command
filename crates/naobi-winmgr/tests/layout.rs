//! The snap geometry, the restore memory and the elevated refusal, over a fake Windows.
use naobi_core::command::{CommandSource, Cx, Query};
use naobi_core::effect::Effect;
use naobi_core::ids::{ActionId, HotkeyId, ItemId};
use naobi_winmgr::plan::{next_display, target, ALL};
use naobi_winmgr::{
    Command, Config, Monitor, Outcome, Rect, SnapCommands, Snapper, WindowInfo, WindowOps,
};
use std::collections::BTreeMap;

fn r(l: i32, t: i32, rt: i32, b: i32) -> Rect {
    Rect::new(l, t, rt, b)
}

fn mon(l: i32, t: i32, rt: i32, b: i32, dpi: u32) -> Monitor {
    Monitor {
        work: r(l, t, rt, b),
        dpi,
    }
}

const ANY: Rect = Rect {
    left: 100,
    top: 100,
    right: 500,
    bottom: 400,
};

// @R-016
#[test]
fn r016_left_half_with_gap() {
    let m = mon(0, 0, 1920, 1040, 96);
    // 8 px from the edges, and half the gap (4) toward the neighbour.
    assert_eq!(
        target(Command::LeftHalf, ANY, &m, 8),
        Some(r(8, 8, 956, 1032))
    );
    assert_eq!(
        target(Command::RightHalf, ANY, &m, 8),
        Some(r(964, 8, 1912, 1032))
    );
    // Two neighbours are exactly one gap apart.
    assert_eq!(964 - 956, 8);
}

// @D-022
#[test]
fn d022_gap_scales_with_the_display_dpi() {
    let m = mon(0, 0, 2880, 1620, 144);
    // 8 logical px at 150% is 12 physical.
    assert_eq!(
        target(Command::LeftHalf, ANY, &m, 8),
        Some(r(12, 12, 1434, 1608))
    );
}

// @D-022
#[test]
fn d022_plan_table_for_all_snap_commands() {
    // No gap, a work area that does not start at 0: the cells are exact.
    let m = mon(100, 50, 1900, 1050, 96);
    let t = |c| target(c, ANY, &m, 0);
    assert_eq!(t(Command::LeftHalf), Some(r(100, 50, 1000, 1050)));
    assert_eq!(t(Command::RightHalf), Some(r(1000, 50, 1900, 1050)));
    assert_eq!(t(Command::TopHalf), Some(r(100, 50, 1900, 550)));
    assert_eq!(t(Command::BottomHalf), Some(r(100, 550, 1900, 1050)));
    assert_eq!(t(Command::LeftThird), Some(r(100, 50, 700, 1050)));
    assert_eq!(t(Command::CenterThird), Some(r(700, 50, 1300, 1050)));
    assert_eq!(t(Command::RightThird), Some(r(1300, 50, 1900, 1050)));
    assert_eq!(t(Command::FirstTwoThirds), Some(r(100, 50, 1300, 1050)));
    assert_eq!(t(Command::LastTwoThirds), Some(r(700, 50, 1900, 1050)));
    assert_eq!(t(Command::TopLeft), Some(r(100, 50, 1000, 550)));
    assert_eq!(t(Command::TopRight), Some(r(1000, 50, 1900, 550)));
    assert_eq!(t(Command::BottomLeft), Some(r(100, 550, 1000, 1050)));
    assert_eq!(t(Command::BottomRight), Some(r(1000, 550, 1900, 1050)));
    // 90% of 1800 x 1000, centered.
    assert_eq!(t(Command::AlmostMaximize), Some(r(190, 100, 1810, 1000)));
    // Center keeps the 400 x 300 size.
    assert_eq!(t(Command::Center), Some(r(800, 400, 1200, 700)));
    // Not placements.
    for c in [Command::Maximize, Command::Restore, Command::NextDisplay] {
        assert_eq!(t(c), None);
    }
}

// @D-022
#[test]
fn d022_center_never_makes_the_window_bigger_than_the_work_area() {
    let m = mon(0, 0, 800, 600, 96);
    let big = r(0, 0, 1000, 900);
    assert_eq!(target(Command::Center, big, &m, 8), Some(r(0, 0, 800, 600)));
}

// @D-022
#[test]
fn d022_thirds_share_the_gap_between_neighbours() {
    let m = mon(0, 0, 1800, 1000, 96);
    let a = target(Command::LeftThird, ANY, &m, 8);
    let b = target(Command::CenterThird, ANY, &m, 8);
    let c = target(Command::RightThird, ANY, &m, 8);
    let (a, b, c) = (
        a.unwrap_or_default(),
        b.unwrap_or_default(),
        c.unwrap_or_default(),
    );
    assert_eq!((b.left - a.right, c.left - b.right), (8, 8));
    assert_eq!((a.left, c.right), (8, 1792));
}

// @R-016
#[test]
fn r016_next_display_keeps_relative_position_mixed_dpi() {
    let a = mon(0, 0, 1920, 1080, 96);
    let b = mon(1920, 0, 4800, 1620, 144);
    // A quarter in from the left and top, half the size.
    let visible = r(480, 270, 1440, 810);
    assert_eq!(
        next_display(visible, &a, &b),
        r(2640, 405, 4080, 1215),
        "fractions of the work area, in the target's physical pixels"
    );
    // And back.
    assert_eq!(next_display(r(2640, 405, 4080, 1215), &b, &a), visible);
}

/// One window on a fake desktop. `moves` records what the commands asked.
struct Fake {
    win: WindowInfo,
    monitors: Vec<Monitor>,
    moves: Vec<String>,
    refuse: bool,
    restored_border: Rect,
}

impl Fake {
    fn new(outer: Rect, border: Rect) -> Self {
        Self {
            win: WindowInfo {
                hwnd: 1,
                title: "Notes".into(),
                outer,
                border,
                monitor: 0,
                elevated: false,
                maximized: false,
            },
            monitors: vec![mon(0, 0, 1920, 1040, 96)],
            moves: Vec::new(),
            refuse: false,
            restored_border: border,
        }
    }
}

impl WindowOps for Fake {
    fn foreground(&mut self) -> Option<WindowInfo> {
        Some(self.win.clone())
    }
    fn monitors(&mut self) -> Vec<Monitor> {
        self.monitors.clone()
    }
    fn move_to(&mut self, hwnd: isize, outer: Rect, dpi: u32) -> bool {
        self.moves.push(format!("move {hwnd} {outer:?} {dpi}"));
        if self.refuse {
            return false;
        }
        self.win.outer = outer;
        let c = (
            outer.left + outer.width() / 2,
            outer.top + outer.height() / 2,
        );
        self.win.monitor = self
            .monitors
            .iter()
            .position(|m| {
                (m.work.left..m.work.right).contains(&c.0)
                    && (m.work.top..m.work.bottom).contains(&c.1)
            })
            .unwrap_or(0);
        true
    }
    fn maximize(&mut self, hwnd: isize) -> bool {
        self.moves.push(format!("maximize {hwnd}"));
        true
    }
    fn unmaximize(&mut self, hwnd: isize) -> bool {
        self.moves.push(format!("unmaximize {hwnd}"));
        // The restored window has the restored border (RK-11).
        self.win.maximized = false;
        self.win.border = self.restored_border;
        true
    }
}

// @R-016
#[test]
fn r016_restore_returns_pre_command_rectangle() {
    let start = r(300, 200, 900, 700);
    let mut ops = Fake::new(start, Rect::default());
    let mut s = Snapper::default();
    assert_eq!(s.run(Command::LeftHalf, &mut ops, 8), Outcome::Moved);
    // A second command does not overwrite where the window was.
    assert_eq!(s.run(Command::TopRight, &mut ops, 8), Outcome::Moved);
    assert_ne!(ops.win.outer, start);
    assert_eq!(s.run(Command::Restore, &mut ops, 8), Outcome::Moved);
    assert_eq!(ops.win.outer, start);
    // Restored once: nothing more to restore until the next command.
    assert_eq!(
        s.run(Command::Restore, &mut ops, 8),
        Outcome::NothingToRestore("Notes".into())
    );
}

// @R-016
#[test]
fn r016_restore_memory_keeps_the_last_64_windows() {
    let mut ops = Fake::new(r(10, 10, 300, 300), Rect::default());
    let mut s = Snapper::default();
    for h in 1..=70 {
        ops.win.hwnd = h;
        ops.win.outer = r(10, 10, 300, 300);
        assert_eq!(s.run(Command::LeftHalf, &mut ops, 8), Outcome::Moved);
    }
    ops.win.hwnd = 1;
    assert_eq!(
        s.run(Command::Restore, &mut ops, 8),
        Outcome::NothingToRestore("Notes".into()),
        "the oldest were forgotten"
    );
    ops.win.hwnd = 70;
    assert_eq!(s.run(Command::Restore, &mut ops, 8), Outcome::Moved);
}

// @R-011
#[test]
fn r011_elevated_window_notice_and_no_change() {
    let mut ops = Fake::new(ANY, Rect::default());
    ops.win.elevated = true;
    let mut s = Snapper::default();
    for c in ALL {
        assert_eq!(s.run(c, &mut ops, 8), Outcome::Elevated("Notes".into()));
    }
    assert!(ops.moves.is_empty(), "nothing was touched: {:?}", ops.moves);
}

// @R-016
#[test]
fn r016_a_refused_move_records_nothing() {
    let mut ops = Fake::new(ANY, Rect::default());
    ops.refuse = true;
    let mut s = Snapper::default();
    assert_eq!(
        s.run(Command::LeftHalf, &mut ops, 8),
        Outcome::Refused("Notes".into())
    );
    ops.refuse = false;
    assert_eq!(
        s.run(Command::Restore, &mut ops, 8),
        Outcome::NothingToRestore("Notes".into())
    );
}

// @R-016
#[test]
fn rk11_target_includes_the_invisible_border_so_the_visible_frame_matches() {
    // Windows 11 style: 7 px invisible resize border on the left, right and bottom.
    let mut ops = Fake::new(r(100, 100, 700, 600), r(7, 0, 7, 7));
    let mut s = Snapper::default();
    assert_eq!(s.run(Command::LeftHalf, &mut ops, 8), Outcome::Moved);
    // The visible frame is (8, 8, 956, 1032); the outer one reaches 7 px further on three sides.
    assert_eq!(ops.win.outer, r(1, 8, 963, 1039));
}

// @R-016
#[test]
fn r016_next_display_moves_to_the_next_one_and_wraps() {
    let mut ops = Fake::new(r(480, 270, 1440, 810), Rect::default());
    ops.monitors = vec![mon(0, 0, 1920, 1080, 96), mon(1920, 0, 4800, 1620, 144)];
    let mut s = Snapper::default();
    assert_eq!(s.run(Command::NextDisplay, &mut ops, 8), Outcome::Moved);
    assert_eq!(ops.win.outer, r(2640, 405, 4080, 1215));
    assert!(ops.moves[0].ends_with("144"), "{:?}", ops.moves);
    assert_eq!(s.run(Command::NextDisplay, &mut ops, 8), Outcome::Moved);
    assert_eq!(ops.win.outer, r(480, 270, 1440, 810));
    // One display: nothing to do.
    let mut one = Fake::new(ANY, Rect::default());
    assert_eq!(
        s.run(Command::NextDisplay, &mut one, 8),
        Outcome::OneDisplay
    );
}

// @R-016
#[test]
fn r016_maximize_uses_the_windows_maximize() {
    let mut ops = Fake::new(ANY, Rect::default());
    let mut s = Snapper::default();
    assert_eq!(s.run(Command::Maximize, &mut ops, 8), Outcome::Moved);
    assert_eq!(ops.moves, vec!["maximize 1".to_owned()]);
    assert_eq!(s.run(Command::Restore, &mut ops, 8), Outcome::Moved);
    assert_eq!(ops.win.outer, ANY);
}

// @D-022
#[test]
fn d022_snap_commands_listed_with_hotkeys() {
    let config = Config::default();
    config.set(
        8,
        BTreeMap::from([("snap_left".to_owned(), "Ctrl+Alt+Left".to_owned())]),
    );
    let source = SnapCommands::new(config);
    let items = source.items(&Query::default());
    assert_eq!(items.len(), ALL.len());
    let left = items.iter().find(|i| i.id.0 == "win.snap_left");
    assert_eq!(left.map(|i| i.subtitle.as_str()), Some("Ctrl+Alt+Left"));
    let third = items.iter().find(|i| i.id.0 == "win.snap_left_third");
    assert_eq!(
        third.map(|i| i.subtitle.as_str()),
        Some(""),
        "no hotkey bound"
    );
    // Running one hides the Launcher and then runs the command as its hotkey would.
    let out = source.run(
        &ItemId("win.snap_left".into()),
        ActionId("run".into()),
        &mut Cx::default(),
    );
    assert_eq!(
        out,
        naobi_core::command::Outcome::Effects(vec![
            Effect::Hide,
            Effect::RunHotkey(HotkeyId("snap_left".into()))
        ])
    );
}

// @D-022
#[test]
fn d022_every_command_id_round_trips() {
    for c in ALL {
        assert_eq!(Command::from_id(c.id()), Some(c));
    }
    assert_eq!(Command::from_id("nope"), None);
}

// @D-022
#[test]
fn d022_rect_grow_and_shrink_are_inverses_per_side() {
    let visible = r(100, 200, 700, 800);
    let border = r(1, 2, 3, 4);
    let outer = visible.grow(&border);
    assert_eq!(outer, r(99, 198, 703, 804));
    assert_eq!(outer.shrink(&border), visible);
    assert_eq!((visible.width(), visible.height()), (600, 600));
}

// @D-022
#[test]
fn d022_the_config_gap_and_the_item_details_are_what_was_set() {
    let config = Config::default();
    assert_eq!(config.gap(), 8);
    config.set(12, BTreeMap::new());
    assert_eq!(config.gap(), 12);
    let source = SnapCommands::new(config);
    let acts = source.actions(&ItemId("win.snap_left".into()));
    assert_eq!(acts.len(), 1);
    assert_eq!(acts[0].label, "Move");
    let titles: std::collections::HashSet<_> = ALL.iter().map(|c| c.title()).collect();
    assert_eq!(titles.len(), ALL.len(), "every command has its own title");
    assert_eq!(Command::LeftHalf.title(), "Snap Left Half");
    assert!(ALL.iter().all(|c| !c.title().is_empty()));
}

// @R-016
#[test]
fn r016_restore_uses_the_dpi_of_the_display_the_rectangle_is_on() {
    for (start, dpi) in [(r(2100, 100, 2700, 600), 144), (r(100, 100, 700, 600), 96)] {
        let mut ops = Fake::new(start, Rect::default());
        ops.monitors = vec![mon(0, 0, 1920, 1080, 96), mon(1920, 0, 4800, 1620, 144)];
        ops.win.monitor = if dpi == 144 { 1 } else { 0 };
        let mut s = Snapper::default();
        assert_eq!(s.run(Command::LeftHalf, &mut ops, 8), Outcome::Moved);
        assert_eq!(s.run(Command::Restore, &mut ops, 8), Outcome::Moved);
        let last = ops.moves.last().cloned().unwrap_or_default();
        assert!(last.ends_with(&format!(" {dpi}")), "{last}");
        assert_eq!(ops.win.outer, start);
    }
}

// @R-016
#[test]
fn r016_exactly_64_windows_are_all_remembered() {
    let mut ops = Fake::new(r(10, 10, 300, 300), Rect::default());
    let mut s = Snapper::default();
    for h in 1..=64 {
        ops.win.hwnd = h;
        ops.win.outer = r(10, 10, 300, 300);
        assert_eq!(s.run(Command::LeftHalf, &mut ops, 8), Outcome::Moved);
    }
    ops.win.hwnd = 1;
    assert_eq!(s.run(Command::Restore, &mut ops, 8), Outcome::Moved);
}

// @R-016
#[test]
fn r016_restore_picks_the_display_that_holds_the_middle_of_a_straddling_window() {
    // The left edge is on the first display, the middle on the second (narrow) one.
    for start in [r(1700, 100, 2300, 600), r(1700, -100, 2300, 400)] {
        let mut ops = Fake::new(start, Rect::default());
        ops.monitors = vec![mon(0, 0, 1920, 1080, 96), mon(1920, 0, 2400, 1080, 144)];
        let mut s = Snapper::default();
        assert_eq!(s.run(Command::LeftHalf, &mut ops, 8), Outcome::Moved);
        assert_eq!(s.run(Command::Restore, &mut ops, 8), Outcome::Moved);
        let last = ops.moves.last().cloned().unwrap_or_default();
        assert!(last.ends_with(" 144"), "{start:?}: {last}");
    }
}

// @R-016
#[test]
fn rk11_a_maximized_window_is_restored_and_measured_again_before_it_is_placed() {
    // Maximized: an 8 px border on every side. Restored: 7 px left, right and bottom.
    let normal = r(300, 200, 900, 700);
    let mut ops = Fake::new(normal, r(8, 8, 8, 8));
    ops.restored_border = r(7, 0, 7, 7);
    ops.win.maximized = true;
    let mut s = Snapper::default();
    assert_eq!(s.run(Command::LeftHalf, &mut ops, 8), Outcome::Moved);
    assert_eq!(ops.moves[0], "unmaximize 1");
    // The visible frame is the plan's, with the restored border around it.
    assert_eq!(ops.win.outer, r(1, 8, 963, 1039));
    // Restore goes back to where the window was, not to the maximized state's rectangle.
    assert_eq!(s.run(Command::Restore, &mut ops, 8), Outcome::Moved);
    assert_eq!(ops.win.outer, normal);
}

// @D-022
#[test]
fn d022_a_huge_gap_setting_is_limited() {
    let config = Config::default();
    config.set(u32::MAX, BTreeMap::new());
    assert_eq!(config.gap(), 64);
}
