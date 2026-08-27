//! Modifier and hotkey parsing.
//!
//! Works with raw Win32 virtual-key codes and `MOD_*` bits (stable numeric values) so the
//! parser stays platform-independent and testable.

pub const MOD_ALT: u32 = 0x0001;
pub const MOD_CONTROL: u32 = 0x0002;
pub const MOD_SHIFT: u32 = 0x0004;
pub const MOD_WIN: u32 = 0x0008;

/// The key held while dragging a window edge.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Modifier {
    Alt,
    Ctrl,
    Shift,
    Win,
}

impl Modifier {
    pub fn parse(s: &str) -> Result<Self, String> {
        match s.trim().to_ascii_lowercase().as_str() {
            "alt" | "option" => Ok(Self::Alt),
            "ctrl" | "control" => Ok(Self::Ctrl),
            "shift" => Ok(Self::Shift),
            "win" | "super" | "meta" | "cmd" => Ok(Self::Win),
            other => Err(format!(
                "unknown modifier '{other}' (expected alt, ctrl, shift or win)"
            )),
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Alt => "Alt",
            Self::Ctrl => "Ctrl",
            Self::Shift => "Shift",
            Self::Win => "Win",
        }
    }
}

/// A global hotkey: `MOD_*` bits plus a virtual-key code, as `RegisterHotKey` expects.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Hotkey {
    pub modifiers: u32,
    pub vk: u32,
}

impl Hotkey {
    /// Parses combos such as `win+alt+c`, `ctrl+shift+f12` or `alt+space` (case-insensitive).
    pub fn parse(s: &str) -> Result<Self, String> {
        let mut modifiers = 0;
        let mut vk = None;
        for part in s.split('+').map(str::trim) {
            let lower = part.to_ascii_lowercase();
            match lower.as_str() {
                "" => return Err(format!("empty key name in hotkey '{s}'")),
                "alt" | "option" => modifiers |= MOD_ALT,
                "ctrl" | "control" => modifiers |= MOD_CONTROL,
                "shift" => modifiers |= MOD_SHIFT,
                "win" | "super" | "meta" | "cmd" => modifiers |= MOD_WIN,
                key => {
                    if vk.replace(key_code(key)?).is_some() {
                        return Err(format!("hotkey '{s}' has more than one non-modifier key"));
                    }
                }
            }
        }
        let vk = vk.ok_or_else(|| format!("hotkey '{s}' needs a key besides the modifiers"))?;
        if modifiers == 0 {
            return Err(format!(
                "hotkey '{s}' needs at least one modifier (alt, ctrl, shift, win)"
            ));
        }
        Ok(Self { modifiers, vk })
    }

    /// Human-readable form for logs, e.g. `Win+Alt+C`.
    pub fn describe(&self) -> String {
        let mut parts = Vec::new();
        for (bit, name) in [
            (MOD_WIN, "Win"),
            (MOD_CONTROL, "Ctrl"),
            (MOD_ALT, "Alt"),
            (MOD_SHIFT, "Shift"),
        ] {
            if self.modifiers & bit != 0 {
                parts.push(name.to_string());
            }
        }
        parts.push(key_name(self.vk));
        parts.join("+")
    }
}

const NAMED_KEYS: &[(&str, u32)] = &[
    ("space", 0x20),
    ("enter", 0x0D),
    ("return", 0x0D),
    ("tab", 0x09),
    ("esc", 0x1B),
    ("escape", 0x1B),
    ("backspace", 0x08),
    ("insert", 0x2D),
    ("ins", 0x2D),
    ("delete", 0x2E),
    ("del", 0x2E),
    ("home", 0x24),
    ("end", 0x23),
    ("pageup", 0x21),
    ("pgup", 0x21),
    ("pagedown", 0x22),
    ("pgdn", 0x22),
    ("left", 0x25),
    ("up", 0x26),
    ("right", 0x27),
    ("down", 0x28),
];

/// Virtual-key code for a lowercase key name: a letter/digit, `f1`..`f24`, or a named key.
fn key_code(name: &str) -> Result<u32, String> {
    if let [c] = name.as_bytes() {
        if c.is_ascii_alphanumeric() {
            return Ok(u32::from(c.to_ascii_uppercase()));
        }
    }
    if let Some(n) = name.strip_prefix('f').and_then(|n| n.parse::<u32>().ok()) {
        if (1..=24).contains(&n) {
            return Ok(0x6F + n); // VK_F1 = 0x70
        }
    }
    NAMED_KEYS
        .iter()
        .find(|(key, _)| *key == name)
        .map(|(_, vk)| *vk)
        .ok_or_else(|| format!("unknown key '{name}'"))
}

fn key_name(vk: u32) -> String {
    if let Some(name) = NAMED_KEYS
        .iter()
        .find(|(_, code)| *code == vk)
        .map(|(n, _)| *n)
    {
        let mut chars = name.chars();
        return chars
            .next()
            .map(|c| c.to_ascii_uppercase())
            .into_iter()
            .chain(chars)
            .collect();
    }
    match vk {
        0x30..=0x39 | 0x41..=0x5A => char::from(vk as u8).to_string(),
        0x70..=0x87 => format!("F{}", vk - 0x6F),
        _ => format!("0x{vk:02X}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_modifiers_case_insensitively() {
        assert_eq!(Modifier::parse("ALT"), Ok(Modifier::Alt));
        assert_eq!(Modifier::parse(" win "), Ok(Modifier::Win));
        assert!(Modifier::parse("hyper").is_err());
    }

    #[test]
    fn parses_default_hotkey() {
        let hk = Hotkey::parse("win+alt+c").unwrap();
        assert_eq!(
            hk,
            Hotkey {
                modifiers: MOD_WIN | MOD_ALT,
                vk: u32::from(b'C')
            }
        );
        assert_eq!(hk.describe(), "Win+Alt+C");
    }

    #[test]
    fn parses_function_and_named_keys() {
        assert_eq!(Hotkey::parse("Ctrl+Shift+F12").unwrap().vk, 0x7B);
        assert_eq!(Hotkey::parse("alt + space").unwrap().vk, 0x20);
        assert_eq!(Hotkey::parse("ctrl+alt+5").unwrap().vk, u32::from(b'5'));
        assert_eq!(Hotkey::parse("ctrl+f").unwrap().vk, u32::from(b'F'));
        assert_eq!(
            Hotkey::parse("ctrl+shift+f12").unwrap().describe(),
            "Ctrl+Shift+F12"
        );
        assert_eq!(Hotkey::parse("alt+pgup").unwrap().describe(), "Alt+Pageup");
    }

    #[test]
    fn rejects_malformed_hotkeys() {
        assert!(Hotkey::parse("c").is_err(), "modifier required");
        assert!(Hotkey::parse("ctrl+alt").is_err(), "key required");
        assert!(Hotkey::parse("ctrl+a+b").is_err(), "single key only");
        assert!(Hotkey::parse("ctrl+f25").is_err());
        assert!(Hotkey::parse("ctrl++c").is_err());
        assert!(
            Hotkey::parse("ctrl+ф").is_err(),
            "non-ASCII keys are not supported"
        );
    }
}
