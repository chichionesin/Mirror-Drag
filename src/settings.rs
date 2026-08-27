//! Feature toggles (persisted under `HKCU\Software\Mirror-Drag`) and the Start-with-Windows entry.

use std::cell::Cell;

use crate::registry;

const KEY: &str = "Software\\Mirror-Drag";
const RUN_KEY: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\Run";
const RUN_VALUE: &str = "Mirror-Drag";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Feature {
    SymmetricResize,
    CenterHotkey,
    ShakeToFind,
}

impl Feature {
    pub const ALL: [Feature; 3] = [
        Feature::SymmetricResize,
        Feature::CenterHotkey,
        Feature::ShakeToFind,
    ];

    fn value_name(self) -> &'static str {
        match self {
            Feature::SymmetricResize => "SymmetricResize",
            Feature::CenterHotkey => "CenterHotkey",
            Feature::ShakeToFind => "ShakeToFind",
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct Flags {
    symmetric_resize: bool,
    center_hotkey: bool,
    shake_to_find: bool,
}

impl Flags {
    const ALL_ON: Flags = Flags {
        symmetric_resize: true,
        center_hotkey: true,
        shake_to_find: true,
    };

    fn get(&self, feature: Feature) -> bool {
        match feature {
            Feature::SymmetricResize => self.symmetric_resize,
            Feature::CenterHotkey => self.center_hotkey,
            Feature::ShakeToFind => self.shake_to_find,
        }
    }

    fn set(&mut self, feature: Feature, on: bool) {
        match feature {
            Feature::SymmetricResize => self.symmetric_resize = on,
            Feature::CenterHotkey => self.center_hotkey = on,
            Feature::ShakeToFind => self.shake_to_find = on,
        }
    }
}

thread_local! {
    static FLAGS: Cell<Flags> = const { Cell::new(Flags::ALL_ON) };
}

/// Reads the persisted toggles; anything not stored yet defaults to enabled.
pub fn load() {
    let mut flags = Flags::ALL_ON;
    for feature in Feature::ALL {
        if let Some(value) = registry::read_dword(KEY, feature.value_name()) {
            flags.set(feature, value != 0);
        }
    }
    FLAGS.set(flags);
}

pub fn is_enabled(feature: Feature) -> bool {
    FLAGS.get().get(feature)
}

/// Flips a toggle and persists it.
pub fn set_enabled(feature: Feature, on: bool) {
    let mut flags = FLAGS.get();
    flags.set(feature, on);
    FLAGS.set(flags);
    if !registry::write_dword(KEY, feature.value_name(), u32::from(on)) {
        log!("settings: could not persist {feature:?}");
    }
}

pub fn autostart_enabled() -> bool {
    registry::value_exists(RUN_KEY, RUN_VALUE)
}

/// Adds/removes the Run entry. The entry re-launches this executable with the arguments it
/// was started with, so custom `--modifier`/`--center-hotkey` choices survive a reboot.
pub fn set_autostart(on: bool) -> bool {
    if on {
        match launch_command() {
            Some(command) => registry::write_string(RUN_KEY, RUN_VALUE, &command),
            None => false,
        }
    } else {
        registry::delete_value(RUN_KEY, RUN_VALUE)
    }
}

fn launch_command() -> Option<String> {
    let exe = std::env::current_exe().ok()?;
    let mut command = format!("\"{}\"", exe.display());
    for arg in std::env::args().skip(1) {
        command.push(' ');
        if arg.contains(' ') {
            command.push('"');
            command.push_str(&arg);
            command.push('"');
        } else {
            command.push_str(&arg);
        }
    }
    Some(command)
}
