//! Key chords as plain data: modifier flags plus a virtual-key number.
use crate::error::ChordError;
use serde::Deserialize;

/// Same values as the Win32 `MOD_*` flags.
pub const ALT: u8 = 1;
pub const CTRL: u8 = 2;
pub const SHIFT: u8 = 4;
pub const WIN: u8 = 8;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize)]
#[serde(try_from = "String")]
pub struct KeyChord {
    pub mods: u8,
    pub vk: u32,
}

impl KeyChord {
    /// Parses "Ctrl+Alt+Left". Case-insensitive; the last part is the key.
    pub fn parse(s: &str) -> Result<Self, ChordError> {
        let bad = |what: &str| ChordError(format!("unknown {what} in chord"));
        let mut parts: Vec<&str> = s.split('+').map(str::trim).collect();
        let key = parts
            .pop()
            .filter(|k| !k.is_empty())
            .ok_or_else(|| bad("key"))?;
        let mut mods = 0;
        for m in parts {
            mods |= match m.to_ascii_lowercase().as_str() {
                "alt" => ALT,
                "ctrl" | "control" => CTRL,
                "shift" => SHIFT,
                "win" => WIN,
                _ => return Err(bad("modifier")),
            };
        }
        let vk = key_code(key).ok_or_else(|| bad("key"))?;
        Ok(Self { mods, vk })
    }
}

impl TryFrom<String> for KeyChord {
    type Error = ChordError;
    fn try_from(s: String) -> Result<Self, ChordError> {
        Self::parse(&s)
    }
}

fn key_code(k: &str) -> Option<u32> {
    let k = k.to_ascii_lowercase();
    let b = k.as_bytes();
    Some(match k.as_str() {
        "backspace" => 0x08,
        "tab" => 0x09,
        "enter" | "return" => 0x0D,
        "esc" | "escape" => 0x1B,
        "space" => 0x20,
        "left" => 0x25,
        "up" => 0x26,
        "right" => 0x27,
        "down" => 0x28,
        _ if b.len() == 1 && b[0].is_ascii_alphanumeric() => u32::from(b[0].to_ascii_uppercase()),
        _ => match k.strip_prefix('f')?.parse::<u32>() {
            Ok(n @ 1..=24) => 0x6F + n,
            _ => return None,
        },
    })
}
