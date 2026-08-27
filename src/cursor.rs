//! "Shake to find": temporarily enlarges the pointer, like macOS.
//!
//! Uses the pointer-size setting introduced in Windows 10 1809
//! (`HKCU\Control Panel\Cursors\CursorBaseSize`, 32 = 100 %) and `SPI_SETCURSORS` to apply it —
//! exactly what the Settings app does. The pre-shake size is parked in our own key so it can be
//! restored at the next start if the process dies while the pointer is enlarged.

use std::cell::Cell;
use std::ptr::null_mut;

use windows_sys::Win32::UI::WindowsAndMessaging::{
    KillTimer, PostMessageW, SetTimer, SystemParametersInfoW, SPIF_SENDCHANGE, SPIF_UPDATEINIFILE,
    SPI_SETCURSORS, WM_APP,
};

use crate::registry;

const CURSORS_KEY: &str = "Control Panel\\Cursors";
const BASE_SIZE_VALUE: &str = "CursorBaseSize";
const DEFAULT_SIZE: u32 = 32;
const MAX_SIZE: u32 = 256;
const SCALE: u32 = 3;
/// How long the pointer stays enlarged after the last shake.
const HOLD_MS: u32 = 1200;
const BACKUP_KEY: &str = "Software\\Mirror-Drag";
const BACKUP_VALUE: &str = "CursorBaseSizeBackup";

/// Thread message posted by the mouse hook when a shake is detected.
pub const WM_SHAKE: u32 = WM_APP + 2;

// (original size, thread timer id) while the pointer is enlarged.
thread_local! {
    static ENLARGED: Cell<Option<(u32, usize)>> = const { Cell::new(None) };
}

/// Asks the message loop to run [`on_shake`] (safe to call from a hook).
pub fn request() {
    unsafe { PostMessageW(null_mut(), WM_SHAKE, 0, 0) };
}

/// Enlarges the pointer, or extends the hold if it is already enlarged.
pub fn on_shake() {
    ENLARGED.with(|state| match state.get() {
        Some((original, timer)) => {
            unsafe { KillTimer(null_mut(), timer) };
            state.set(Some((original, start_timer())));
        }
        None => {
            let original =
                registry::read_dword(CURSORS_KEY, BASE_SIZE_VALUE).unwrap_or(DEFAULT_SIZE);
            registry::write_dword(BACKUP_KEY, BACKUP_VALUE, original);
            let size = (original * SCALE).min(MAX_SIZE);
            log!("shake: pointer {original} -> {size}");
            apply_size(size);
            state.set(Some((original, start_timer())));
        }
    });
}

/// Handles a `WM_TIMER` from the thread queue; returns `true` if it was ours.
pub fn on_timer(id: usize) -> bool {
    ENLARGED.with(|state| match state.get() {
        Some((original, timer)) if timer == id => {
            unsafe { KillTimer(null_mut(), timer) };
            state.set(None);
            restore(original);
            true
        }
        _ => false,
    })
}

/// Restores the pointer if a previous run died while it was enlarged.
pub fn restore_after_crash() {
    if let Some(original) = registry::read_dword(BACKUP_KEY, BACKUP_VALUE) {
        log!("shake: restoring pointer size {original} left over from a previous run");
        restore(original);
    }
}

/// Restores the pointer if we are exiting while it is enlarged.
pub fn shutdown() {
    ENLARGED.with(|state| {
        if let Some((original, timer)) = state.take() {
            unsafe { KillTimer(null_mut(), timer) };
            restore(original);
        }
    });
}

fn start_timer() -> usize {
    unsafe { SetTimer(null_mut(), 0, HOLD_MS, None) }
}

fn restore(original: u32) {
    log!("shake: pointer back to {original}");
    apply_size(original);
    registry::delete_value(BACKUP_KEY, BACKUP_VALUE);
}

fn apply_size(size: u32) {
    if !registry::write_dword(CURSORS_KEY, BASE_SIZE_VALUE, size) {
        log!("shake: could not write {BASE_SIZE_VALUE}");
        return;
    }
    unsafe {
        SystemParametersInfoW(
            SPI_SETCURSORS,
            0,
            null_mut(),
            SPIF_UPDATEINIFILE | SPIF_SENDCHANGE,
        )
    };
}
