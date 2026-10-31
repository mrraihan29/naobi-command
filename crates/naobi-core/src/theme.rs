//! Live theme and reduced motion (R-007). The OS read sits behind `ThemeSource`; the rule for when the
//! look changes is plain data, so tests drive it with `FixedTheme` and a `ThemeChanged` event.
use std::sync::Mutex;

use crate::event::AppEvent;
use crate::ui::Theme;

/// What the Launcher draws with: the palette and the motion setting.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Look {
    pub theme: Theme,
    pub reduced_motion: bool,
}

/// Where the look comes from: the OS in the app, a value set by hand in tests.
pub trait ThemeSource: Send + Sync {
    fn current(&self) -> Theme;
    fn reduced_motion(&self) -> bool;
}

/// The look the source reports now.
pub fn read(src: &dyn ThemeSource) -> Look {
    Look {
        theme: src.current(),
        reduced_motion: src.reduced_motion(),
    }
}

/// The new look when `ev` is a theme change that moved it away from `last`; None otherwise.
pub fn follow(ev: &AppEvent, src: &dyn ThemeSource, last: Look) -> Option<Look> {
    if *ev != AppEvent::ThemeChanged {
        return None;
    }
    let now = read(src);
    (now != last).then_some(now)
}

/// A source whose look the caller sets.
pub struct FixedTheme(Mutex<Look>);

impl FixedTheme {
    pub fn new(look: Look) -> Self {
        Self(Mutex::new(look))
    }

    pub fn set(&self, look: Look) {
        *self.0.lock().unwrap_or_else(|e| e.into_inner()) = look;
    }

    fn get(&self) -> Look {
        *self.0.lock().unwrap_or_else(|e| e.into_inner())
    }
}

impl ThemeSource for FixedTheme {
    fn current(&self) -> Theme {
        self.get().theme
    }

    fn reduced_motion(&self) -> bool {
        self.get().reduced_motion
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PAPER: Look = Look {
        theme: Theme::Paper,
        reduced_motion: false,
    };

    // @R-007
    #[test]
    fn r007_theme_switch_without_restart() {
        let os = FixedTheme::new(PAPER);
        let mut last = read(&os);
        os.set(Look {
            theme: Theme::Stone,
            ..PAPER
        });
        // Only the forwarded setting change moves the look; any other event leaves it alone.
        assert_eq!(follow(&AppEvent::Resume, &os, last), None);
        let next = follow(&AppEvent::ThemeChanged, &os, last);
        assert_eq!(next.map(|l| l.theme), Some(Theme::Stone));
        last = next.expect("ThemeChanged moves the look to stone");
        assert_eq!(
            follow(&AppEvent::ThemeChanged, &os, last),
            None,
            "the same look is not a change"
        );
    }

    #[test]
    fn reduced_motion_follows_the_source_without_restart() {
        let os = FixedTheme::new(PAPER);
        let last = read(&os);
        assert!(!last.reduced_motion);
        os.set(Look {
            reduced_motion: true,
            ..PAPER
        });
        let next = follow(&AppEvent::ThemeChanged, &os, last).expect("motion changed");
        assert!(next.reduced_motion);
        assert_eq!(next.theme, Theme::Paper);
    }
}
