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

/// The text `parse` reads back: "Ctrl+Alt+Left". A key outside the parser's table prints as "0x.."
/// and does not parse, which only a hand-built chord can have.
impl std::fmt::Display for KeyChord {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for (flag, name) in [(CTRL, "Ctrl"), (ALT, "Alt"), (SHIFT, "Shift"), (WIN, "Win")] {
            if self.mods & flag != 0 {
                write!(f, "{name}+")?;
            }
        }
        match self.vk {
            0x08 => write!(f, "Backspace"),
            0x09 => write!(f, "Tab"),
            0x0D => write!(f, "Enter"),
            0x1B => write!(f, "Esc"),
            0x20 => write!(f, "Space"),
            0x25 => write!(f, "Left"),
            0x26 => write!(f, "Up"),
            0x27 => write!(f, "Right"),
            0x28 => write!(f, "Down"),
            0xBE => write!(f, "."),
            v @ (0x30..=0x39 | 0x41..=0x5A) => write!(f, "{}", char::from(v as u8)),
            v @ 0x70..=0x87 => write!(f, "F{}", v - 0x6F),
            v => write!(f, "0x{v:X}"),
        }
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
        "." | "period" => 0xBE,
        _ if b.len() == 1 && b[0].is_ascii_alphanumeric() => u32::from(b[0].to_ascii_uppercase()),
        _ => match k.strip_prefix('f')?.parse::<u32>() {
            Ok(n @ 1..=24) => 0x6F + n,
            _ => return None,
        },
    })
}
