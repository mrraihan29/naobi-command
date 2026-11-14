//! The toggle and the border style, over a fake Windows.
use naobi_aot::{
    border_color, style, AotCommands, Config, Outcome, Style, Target, Topmost, TopmostOps, DARK,
    LIGHT,
};
use naobi_core::command::{CommandSource, Cx, Query};
use naobi_core::effect::Effect;
use naobi_core::ids::{ActionId, HotkeyId, ItemId};
use naobi_core::ui::Theme;

#[derive(Default)]
struct Fake {
    front: Option<Target>,
    topmost: bool,
    refuse: bool,
    calls: Vec<String>,
}

impl TopmostOps for Fake {
    fn foreground(&mut self) -> Option<Target> {
        self.front.clone()
    }
    fn is_topmost(&mut self, _hwnd: isize) -> bool {
        self.topmost
    }
    fn set_topmost(&mut self, hwnd: isize, on: bool) -> bool {
        self.calls.push(format!("set {hwnd} {on}"));
        if self.refuse {
            return false;
        }
        self.topmost = on;
        true
    }
    fn follow(&mut self, hwnd: isize, on: bool) {
        self.calls.push(format!("follow {hwnd} {on}"));
    }
    fn set_style(&mut self, _style: Style) {}
}

fn window(elevated: bool) -> Fake {
    Fake {
        front: Some(Target {
            hwnd: 5,
            title: "Notes".into(),
            elevated,
        }),
        ..Fake::default()
    }
}

// @R-018
#[test]
fn r018_toggle_twice_returns_non_topmost() {
    let mut ops = window(false);
    let mut t = Topmost::default();
    assert_eq!(t.toggle(&mut ops), Outcome::On);
    assert!(ops.topmost);
    assert_eq!(t.toggle(&mut ops), Outcome::Off);
    assert!(!ops.topmost);
    // The border starts following on the first toggle and stops on the second.
    assert_eq!(
        ops.calls,
        [
            "set 5 true",
            "follow 5 true",
            "set 5 false",
            "follow 5 false"
        ]
    );
}

// @R-018
#[test]
fn r018_a_window_that_is_already_topmost_is_toggled_off() {
    let mut ops = window(false);
    ops.topmost = true;
    assert_eq!(Topmost::default().toggle(&mut ops), Outcome::Off);
    assert!(!ops.topmost);
}

// @R-011
#[test]
fn r011_pin_elevated_window_notice_and_no_change() {
    let mut ops = window(true);
    assert_eq!(
        Topmost::default().toggle(&mut ops),
        Outcome::Elevated("Notes".into())
    );
    assert!(ops.calls.is_empty(), "nothing was touched: {:?}", ops.calls);
}

// @R-018
#[test]
fn r018_no_window_and_a_refusal_change_nothing() {
    let mut none = Fake::default();
    assert_eq!(Topmost::default().toggle(&mut none), Outcome::NoWindow);
    let mut refused = window(false);
    refused.refuse = true;
    assert_eq!(
        Topmost::default().toggle(&mut refused),
        Outcome::Refused("Notes".into())
    );
    assert!(
        !refused.calls.iter().any(|c| c.starts_with("follow")),
        "no border for a window that was not pinned"
    );
}

// @R-018
#[test]
fn r018_border_color_follows_windows_theme() {
    assert_eq!(border_color("auto", Theme::Paper), 0x914A3B);
    assert_eq!(border_color("auto", Theme::Stone), 0xE3BDA9);
    assert_eq!((LIGHT, DARK), (0x914A3B, 0xE3BDA9));
    // A custom color wins in both themes; anything malformed is "auto".
    assert_eq!(border_color("#112233", Theme::Stone), 0x112233);
    for bad in ["", "red", "#12345", "#1234567", "#GGGGGG", "112233"] {
        assert_eq!(border_color(bad, Theme::Paper), LIGHT, "{bad}");
    }
    // `aot.border = false` hides it.
    assert_eq!(
        style(false, "auto", Theme::Paper),
        Style {
            show: false,
            color: LIGHT
        }
    );
}

// @R-018
#[test]
fn r018_toggle_always_on_top_is_a_launcher_command_with_the_topmost_hotkey() {
    let config = Config::default();
    config.set("Win+Ctrl+T".into());
    let source = AotCommands::new(config);
    let items = source.items(&Query::default());
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].title, "Toggle Always on Top");
    assert_eq!(items[0].subtitle, "Win+Ctrl+T");
    let out = source.run(
        &ItemId("aot.toggle".into()),
        ActionId("run".into()),
        &mut Cx::default(),
    );
    assert_eq!(
        out,
        naobi_core::command::Outcome::Effects(vec![
            Effect::Hide,
            Effect::RunHotkey(HotkeyId("topmost".into()))
        ])
    );
    assert_eq!(source.actions(&items[0].id).len(), 1);
}
