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

/// Presses and releases Ctrl+Alt+`vk` (the window commands' hotkeys).
pub fn ctrl_alt(vk: u16) {
    let k = VIRTUAL_KEY(vk);
    let seq = [
        key(VK_LCONTROL, false),
        key(VK_MENU, false),
        key(k, false),
        key(k, true),
        key(VK_MENU, true),
        key(VK_LCONTROL, true),
    ];
    // SAFETY: SendInput reads `seq`, a slice of fully initialised INPUT values, with their true size.
    unsafe { SendInput(&seq, std::mem::size_of::<INPUT>() as i32) };
}

/// Presses and releases Win+Ctrl+T (the always-on-top hotkey).
pub fn win_ctrl_t() {
    let (win, t) = (VIRTUAL_KEY(0x5B), VIRTUAL_KEY(0x54));
    let seq = [
        key(VK_LCONTROL, false),
        key(win, false),
        key(t, false),
        key(t, true),
        key(win, true),
        key(VK_LCONTROL, true),
    ];
    // SAFETY: SendInput reads `seq`, a slice of fully initialised INPUT values, with their true size.
    unsafe { SendInput(&seq, std::mem::size_of::<INPUT>() as i32) };
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

/// Presses and releases Win+Shift+C (the color picker hotkey).
pub fn win_shift_c() {
    let (win, c) = (VIRTUAL_KEY(0x5B), VIRTUAL_KEY(0x43));
    let seq = [
        key(VK_SHIFT, false),
        key(win, false),
        key(c, false),
        key(c, true),
        key(win, true),
        key(VK_SHIFT, true),
    ];
    // SAFETY: SendInput reads `seq`, a slice of fully initialised INPUT values, with their true size.
    unsafe { SendInput(&seq, std::mem::size_of::<INPUT>() as i32) };
}

/// Moves the pointer to screen pixel `(x, y)` and clicks the left button there.
pub fn click_at(x: i32, y: i32) {
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        INPUT_MOUSE, MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP, MOUSEINPUT,
    };
    let button = |flags| INPUT {
        r#type: INPUT_MOUSE,
        Anonymous: INPUT_0 {
            mi: MOUSEINPUT {
                dwFlags: flags,
                ..Default::default()
            },
        },
    };
    // SAFETY: SetCursorPos takes plain values; SendInput reads a slice of initialised INPUT values.
    unsafe {
        let _ = windows::Win32::UI::WindowsAndMessaging::SetCursorPos(x, y);
        // Let the move reach the overlay before the button does.
        std::thread::sleep(std::time::Duration::from_millis(150));
        SendInput(
            &[button(MOUSEEVENTF_LEFTDOWN), button(MOUSEEVENTF_LEFTUP)],
            std::mem::size_of::<INPUT>() as i32,
        );
    }
}

/// Presses the left button at screen pixel `from`, moves the pointer to `to` in steps and lets go.
pub fn drag(from: (i32, i32), to: (i32, i32)) {
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        INPUT_MOUSE, MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP, MOUSEINPUT,
    };
    use windows::Win32::UI::WindowsAndMessaging::SetCursorPos;
    let button = |flags| INPUT {
        r#type: INPUT_MOUSE,
        Anonymous: INPUT_0 {
            mi: MOUSEINPUT {
                dwFlags: flags,
                ..Default::default()
            },
        },
    };
    let pause = || std::thread::sleep(std::time::Duration::from_millis(30));
    // SAFETY: SetCursorPos takes plain values; SendInput reads a slice of initialised INPUT values.
    unsafe {
        let _ = SetCursorPos(from.0, from.1);
        std::thread::sleep(std::time::Duration::from_millis(150));
        SendInput(
            &[button(MOUSEEVENTF_LEFTDOWN)],
            std::mem::size_of::<INPUT>() as i32,
        );
        for i in 1..=20 {
            let _ = SetCursorPos(
                from.0 + (to.0 - from.0) * i / 20,
                from.1 + (to.1 - from.1) * i / 20,
            );
            pause();
        }
        SendInput(
            &[button(MOUSEEVENTF_LEFTUP)],
            std::mem::size_of::<INPUT>() as i32,
        );
    }
}
