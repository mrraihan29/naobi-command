//! Key injection for tier 2 tests and naobi-perf. Compiled only with feature `perf`.
use windows::Win32::UI::Input::KeyboardAndMouse::{
    SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP, VIRTUAL_KEY,
    VK_LCONTROL, VK_MENU, VK_SPACE,
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
