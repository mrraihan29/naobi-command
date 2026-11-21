//! Global hotkeys through `RegisterHotKey` only (ADR-0004). One id per settings chord; a chord that
//! another process owns becomes an event, and the other chords still register.
use std::sync::OnceLock;

use naobi_core::chord::KeyChord;
use naobi_core::event::AppEvent;
use naobi_core::ids::HotkeyId;
use naobi_core::settings::{Settings, OPTIONAL_HOTKEYS};
use windows::Win32::Foundation::HWND;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    RegisterHotKey, UnregisterHotKey, HOT_KEY_MODIFIERS, MOD_NOREPEAT,
};

/// Hotkey names in id order: id = position + 1. The settings parser accepts only these names.
fn names() -> &'static [String] {
    static NAMES: OnceLock<Vec<String>> = OnceLock::new();
    NAMES.get_or_init(|| {
        Settings::default()
            .hotkeys
            .into_keys()
            .chain(OPTIONAL_HOTKEYS.into_iter().map(str::to_owned))
            .collect()
    })
}

/// The hotkey a `WM_HOTKEY` id stands for.
pub fn name_for_id(id: usize) -> Option<HotkeyId> {
    names().get(id.checked_sub(1)?).map(|n| HotkeyId(n.clone()))
}

/// The OS side of registration, so the logic runs in tests without Windows.
pub trait Registrar {
    fn register(&mut self, id: i32, chord: KeyChord) -> bool;
    fn unregister(&mut self, id: i32);
}

/// Registers on the platform window; call it only on the platform thread.
pub struct Win32 {
    pub hwnd: HWND,
}

impl Registrar for Win32 {
    fn register(&mut self, id: i32, chord: KeyChord) -> bool {
        let mods = HOT_KEY_MODIFIERS(u32::from(chord.mods)) | MOD_NOREPEAT;
        // SAFETY: plain values; the window is the platform window that this thread owns.
        unsafe { RegisterHotKey(Some(self.hwnd), id, mods, chord.vk).is_ok() }
    }

    fn unregister(&mut self, id: i32) {
        // SAFETY: plain values; unregistering an id that is not registered is harmless.
        let _ = unsafe { UnregisterHotKey(Some(self.hwnd), id) };
    }
}

/// The chords of the settings file with their ids, and whether Pause is on.
pub struct Registry {
    entries: Vec<(i32, HotkeyId, KeyChord)>,
    paused: bool,
}

impl Registry {
    pub fn new(settings: &Settings) -> Self {
        let mut entries = Vec::new();
        for (name, chord) in &settings.hotkeys {
            if let Some(index) = names().iter().position(|n| n == name) {
                entries.push((index as i32 + 1, HotkeyId(name.clone()), *chord));
            }
        }
        Self {
            entries,
            paused: false,
        }
    }

    /// The settings changed: releases the old chords and registers the new ones, unless paused. A
    /// chord that did not change is released and taken again, which is harmless.
    pub fn reconfigure(&mut self, r: &mut impl Registrar, settings: &Settings) -> Vec<AppEvent> {
        for (id, _, _) in &self.entries {
            r.unregister(*id);
        }
        let paused = self.paused;
        *self = Self::new(settings);
        self.paused = paused;
        self.register_all(r)
    }

    pub fn is_paused(&self) -> bool {
        self.paused
    }

    /// What a registration pass that returned `conflicts` leaves held. None while paused.
    pub fn held(&self, conflicts: &[AppEvent]) -> Option<AppEvent> {
        (!self.paused).then(|| {
            AppEvent::HotkeysHeld(
                self.entries
                    .iter()
                    .map(|(_, name, _)| name.clone())
                    .filter(|name| !conflicts.contains(&AppEvent::HotkeyConflict(name.clone())))
                    .collect(),
            )
        })
    }

    /// Registers every chord unless paused. Returns one `HotkeyConflict` per chord that was refused.
    pub fn register_all(&mut self, r: &mut impl Registrar) -> Vec<AppEvent> {
        if self.paused {
            return Vec::new();
        }
        self.apply(r)
    }

    /// Pause unregisters every id; resume registers them again (also the retry after sleep).
    pub fn set_paused(&mut self, r: &mut impl Registrar, paused: bool) -> Vec<AppEvent> {
        self.paused = paused;
        if paused {
            for (id, _, _) in &self.entries {
                r.unregister(*id);
            }
            return Vec::new();
        }
        self.apply(r)
    }

    fn apply(&self, r: &mut impl Registrar) -> Vec<AppEvent> {
        let mut conflicts = Vec::new();
        for (id, name, chord) in &self.entries {
            // Unregister first so a retry after resume does not see our own old registration.
            r.unregister(*id);
            if !r.register(*id, *chord) {
                conflicts.push(AppEvent::HotkeyConflict(name.clone()));
            }
        }
        conflicts
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    /// Fake OS: `taken` chords belong to another process, `live` ids are what we hold.
    // @R-008
    #[test]
    fn r008_reconfigure_moves_chords_and_keeps_pause() {
        let mut fake = Fake::default();
        let mut reg = Registry::new(&Settings::default());
        assert!(reg.register_all(&mut fake).is_empty());
        let live_before = fake.live.len();
        // The launcher chord moves to a key another process owns: one conflict, the rest stay.
        let mut next = Settings::default();
        next.hotkeys
            .insert("launcher".into(), KeyChord::parse("Alt+F9").expect("chord"));
        fake.taken.insert(0x78);
        let events = reg.reconfigure(&mut fake, &next);
        assert_eq!(
            events,
            vec![AppEvent::HotkeyConflict(HotkeyId("launcher".into()))]
        );
        assert_eq!(fake.live.len(), live_before - 1);
        // Paused stays paused through a reload: nothing is registered until resume.
        reg.set_paused(&mut fake, true);
        assert!(reg.reconfigure(&mut fake, &Settings::default()).is_empty());
        assert!(fake.live.is_empty() && reg.is_paused());
        assert!(reg.set_paused(&mut fake, false).is_empty());
        assert_eq!(fake.live.len(), live_before);
    }

    #[derive(Default)]
    struct Fake {
        taken: BTreeSet<u32>,
        live: BTreeSet<i32>,
    }

    impl Registrar for Fake {
        fn register(&mut self, id: i32, chord: KeyChord) -> bool {
            if self.taken.contains(&chord.vk) {
                return false;
            }
            self.live.insert(id);
            true
        }
        fn unregister(&mut self, id: i32) {
            self.live.remove(&id);
        }
    }

    #[test]
    fn ids_follow_the_default_table_and_names_round_trip() {
        assert_eq!(name_for_id(0), None);
        let reg = Registry::new(&Settings::default());
        assert_eq!(reg.entries.len(), 4);
        let (id, name, chord) = &reg.entries[0];
        assert_eq!((*id, name.0.as_str()), (1, "clipboard"));
        assert_eq!(*chord, Settings::default().hotkeys["clipboard"]);
        assert_eq!(name_for_id(1), Some(HotkeyId("clipboard".into())));
    }

    #[test]
    fn optional_window_switcher_hotkey_registers_when_configured() {
        let mut fake = Fake::default();
        let parsed = Settings::parse("[hotkeys]\nwindow_switcher = \"Alt+Shift+W\"\n").unwrap();
        let mut reg = Registry::new(&parsed.settings);
        assert!(reg.register_all(&mut fake).is_empty());
        assert_eq!(fake.live.len(), 5);
        let id = reg
            .entries
            .iter()
            .find(|(_, name, _)| name.0 == "window_switcher")
            .map(|(id, _, _)| *id)
            .expect("window_switcher registered");
        assert_eq!(
            name_for_id(id as usize),
            Some(HotkeyId("window_switcher".into()))
        );
    }

    #[test]
    fn optional_scratchpad_hotkey_registers_when_configured() {
        let mut fake = Fake::default();
        let parsed = Settings::parse("[hotkeys]\nscratchpad = \"Alt+Shift+N\"\n").unwrap();
        let mut reg = Registry::new(&parsed.settings);
        assert!(reg.register_all(&mut fake).is_empty());
        assert_eq!(fake.live.len(), 5);
        let id = reg
            .entries
            .iter()
            .find(|(_, name, _)| name.0 == "scratchpad")
            .map(|(id, _, _)| *id)
            .expect("scratchpad registered");
        assert_eq!(
            name_for_id(id as usize),
            Some(HotkeyId("scratchpad".into()))
        );
    }

    // @R-008
    #[test]
    fn r008_conflict_is_reported_and_other_chords_still_register() {
        let mut reg = Registry::new(&Settings::default());
        let mut fake = Fake {
            taken: BTreeSet::from([0x20]), // Space: Ctrl+Space is owned by another process
            ..Fake::default()
        };
        let events = reg.register_all(&mut fake);
        assert_eq!(
            events,
            vec![AppEvent::HotkeyConflict(HotkeyId("launcher".into()))]
        );
        assert_eq!(fake.live.len(), 3);
    }

    // @R-008
    #[test]
    fn r008_refused_chord_is_retried_on_resume() {
        let mut reg = Registry::new(&Settings::default());
        let mut fake = Fake {
            taken: BTreeSet::from([0x20]),
            ..Fake::default()
        };
        assert_eq!(reg.register_all(&mut fake).len(), 1);
        // The other process lets go of Ctrl+Space while the machine sleeps.
        fake.taken.clear();
        assert!(reg.register_all(&mut fake).is_empty());
        assert_eq!(fake.live.len(), 4);
    }

    // @R-022
    #[test]
    fn r022_pause_unregisters_every_id_and_resume_registers_them_again() {
        let mut reg = Registry::new(&Settings::default());
        let mut fake = Fake::default();
        reg.register_all(&mut fake);
        assert_eq!(fake.live.len(), 4);

        assert!(reg.set_paused(&mut fake, true).is_empty());
        assert!(fake.live.is_empty());
        assert!(reg.register_all(&mut fake).is_empty());
        assert!(fake.live.is_empty(), "no registration while paused");

        assert!(reg.set_paused(&mut fake, false).is_empty());
        assert_eq!(fake.live.len(), 4);
    }

    #[test]
    fn modifier_bits_are_the_win32_mod_values() {
        use naobi_core::chord::{ALT, CTRL, SHIFT, WIN};
        use windows::Win32::UI::Input::KeyboardAndMouse::{
            MOD_ALT, MOD_CONTROL, MOD_SHIFT, MOD_WIN,
        };
        assert_eq!(MOD_ALT.0, u32::from(ALT));
        assert_eq!(MOD_CONTROL.0, u32::from(CTRL));
        assert_eq!(MOD_SHIFT.0, u32::from(SHIFT));
        assert_eq!(MOD_WIN.0, u32::from(WIN));
    }
}
