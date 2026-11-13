//! Key injection for tier 2 tests and naobi-perf. Compiled only with feature `perf`.
use windows::Win32::UI::Input::KeyboardAndMouse::{
    RegisterHotKey, SendInput, UnregisterHotKey, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT,
    KEYEVENTF_KEYUP, KEYEVENTF_UNICODE, MOD_ALT, VIRTUAL_KEY, VK_LCONTROL, VK_MENU, VK_RETURN,
    VK_SHIFT, VK_SPACE,
};

fn key(vk: VIRTUAL_KEY, up: bool) -> INPUT {
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: vk,
                dwFlags: if up {
                    KEYEVENTF_KEYUP
                } else {
                    Default::default()
                },
                ..Default::default()
            },
        },
    }
}

/// Presses and releases Alt+Space (the launcher hotkey).
pub fn alt_space() {
    let seq = [
        key(VK_MENU, false),
        key(VK_SPACE, false),
        key(VK_SPACE, true),
        key(VK_MENU, true),
    ];
    // SAFETY: SendInput reads `seq`, a slice of fully initialised INPUT values, with their true size.
    unsafe { SendInput(&seq, std::mem::size_of::<INPUT>() as i32) };
}

/// Presses and releases Left Ctrl+Space (the owner's PowerToys Command Palette hotkey, NCMD-6).
pub fn ctrl_space() {
    let seq = [
        key(VK_LCONTROL, false),
        key(VK_SPACE, false),
        key(VK_SPACE, true),
        key(VK_LCONTROL, true),
    ];
    // SAFETY: SendInput reads `seq`, a slice of fully initialised INPUT values, with their true size.
    unsafe { SendInput(&seq, std::mem::size_of::<INPUT>() as i32) };
}

/// Types `text` as Unicode key events, one character at a time (tier 2 tests).
pub fn text(text: &str) {
    let mut seq = Vec::new();
    for unit in text.encode_utf16() {
        for up in [false, true] {
            seq.push(INPUT {
                r#type: INPUT_KEYBOARD,
                Anonymous: INPUT_0 {
                    ki: KEYBDINPUT {
                        wScan: unit,
                        dwFlags: if up {
                            KEYEVENTF_UNICODE | KEYEVENTF_KEYUP
                        } else {
                            KEYEVENTF_UNICODE
                        },
                        ..Default::default()
                    },
                },
            });
        }
    }
    // SAFETY: SendInput reads `seq`, a slice of fully initialised INPUT values, with their true size.
    unsafe { SendInput(&seq, std::mem::size_of::<INPUT>() as i32) };
}

/// Presses and releases Enter.
pub fn enter() {
    let seq = [key(VK_RETURN, false), key(VK_RETURN, true)];
    // SAFETY: SendInput reads `seq`, a slice of fully initialised INPUT values, with their true size.
    unsafe { SendInput(&seq, std::mem::size_of::<INPUT>() as i32) };
}

/// A hotkey held by this thread, released on drop. Tests use it to make another process's chord
/// "taken", so the app under test reports a conflict.
pub struct HeldChord(i32);

impl Drop for HeldChord {
    fn drop(&mut self) {
        // SAFETY: unregisters the id this value registered on this thread.
        let _ = unsafe { UnregisterHotKey(None, self.0) };
    }
}

/// Registers Alt+Space on the calling thread. None when someone else already holds it.
pub fn hold_alt_space() -> Option<HeldChord> {
    const ID: i32 = 0x7E57;
    // SAFETY: plain values; no window, so the chord belongs to this thread's message queue.
    unsafe { RegisterHotKey(None, ID, MOD_ALT, VK_SPACE.0.into()) }
        .ok()
        .map(|()| HeldChord(ID))
}

/// Presses and releases Alt+Shift+V (the clipboard history hotkey).
pub fn alt_shift_v() {
    let v = VIRTUAL_KEY(0x56);
    let seq = [
        key(VK_MENU, false),
        key(VK_SHIFT, false),
        key(v, false),
        key(v, true),
        key(VK_SHIFT, true),
        key(VK_MENU, true),
    ];
    // SAFETY: SendInput reads `seq`, a slice of fully initialised INPUT values, with their true size.
    unsafe { SendInput(&seq, std::mem::size_of::<INPUT>() as i32) };
}
