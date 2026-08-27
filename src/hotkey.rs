//! The center-window hotkey (`RegisterHotKey`), switchable at runtime from the tray menu.

use std::cell::Cell;
use std::ptr::null_mut;

use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    RegisterHotKey, UnregisterHotKey, MOD_NOREPEAT,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{MSG, WM_HOTKEY};

use crate::keys::Hotkey;
use crate::ui::last_error;

const ID: i32 = 1;

thread_local! {
    static HOTKEY: Cell<Option<Hotkey>> = const { Cell::new(None) };
    static REGISTERED: Cell<bool> = const { Cell::new(false) };
}

/// Remembers which combination to use; `None` means the feature is unavailable.
pub fn configure(hotkey: Option<Hotkey>) {
    HOTKEY.set(hotkey);
}

pub fn available() -> bool {
    HOTKEY.get().is_some()
}

pub fn describe() -> String {
    HOTKEY
        .get()
        .map_or_else(|| "disabled".to_string(), |hotkey| hotkey.describe())
}

/// Registers or unregisters the hotkey; a no-op when already in the requested state.
pub fn set_enabled(on: bool) -> Result<(), String> {
    let Some(hotkey) = HOTKEY.get() else {
        return Ok(());
    };
    if on == REGISTERED.get() {
        return Ok(());
    }
    let ok = unsafe {
        if on {
            RegisterHotKey(null_mut(), ID, hotkey.modifiers | MOD_NOREPEAT, hotkey.vk)
        } else {
            UnregisterHotKey(null_mut(), ID)
        }
    } != 0;
    if !ok {
        return Err(format!(
            "Could not register the center-window hotkey {} (error {}); another program probably uses \
             it. Choose another key with --center-hotkey.",
            hotkey.describe(),
            last_error()
        ));
    }
    REGISTERED.set(on);
    Ok(())
}

/// `true` for the `WM_HOTKEY` posted for our hotkey.
pub fn matches(msg: &MSG) -> bool {
    msg.message == WM_HOTKEY && msg.wParam == ID as usize
}

pub fn shutdown() {
    let _ = set_enabled(false);
}
