//! Output for a window-less process: an optional debug console, message boxes and logging.

use std::ptr::null_mut;
use std::sync::atomic::{AtomicBool, Ordering};

use windows_sys::Win32::Foundation::GetLastError;
use windows_sys::Win32::System::Console::{AllocConsole, AttachConsole, ATTACH_PARENT_PROCESS};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    MessageBoxW, IDYES, MB_ICONERROR, MB_ICONINFORMATION, MB_ICONQUESTION, MB_OK, MB_SETFOREGROUND,
    MB_YESNO, MESSAGEBOX_STYLE,
};

pub const APP_NAME: &str = "Mirror-Drag";

static HAS_CONSOLE: AtomicBool = AtomicBool::new(false);
static VERBOSE: AtomicBool = AtomicBool::new(false);

/// Prints a debug line when `--console` is active.
macro_rules! log {
    ($($arg:tt)*) => {
        if $crate::ui::verbose() {
            eprintln!($($arg)*);
        }
    };
}

pub fn verbose() -> bool {
    VERBOSE.load(Ordering::Relaxed)
}

/// NUL-terminated UTF-16 for the Win32 `W` APIs.
pub fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

pub fn last_error() -> u32 {
    unsafe { GetLastError() }
}

/// Attaches to the parent console when launched from a terminal so text output is visible.
pub fn attach_parent_console() -> bool {
    let ok = unsafe { AttachConsole(ATTACH_PARENT_PROCESS) } != 0;
    if ok {
        HAS_CONSOLE.store(true, Ordering::Relaxed);
    }
    ok
}

/// Opens a console for debug output (`--console`).
pub fn open_console() {
    if !HAS_CONSOLE.load(Ordering::Relaxed) && unsafe { AllocConsole() } != 0 {
        HAS_CONSOLE.store(true, Ordering::Relaxed);
    }
    VERBOSE.store(true, Ordering::Relaxed);
}

/// Shows `text` on the console when there is one, otherwise in a message box.
pub fn report(text: &str, is_error: bool) {
    if HAS_CONSOLE.load(Ordering::Relaxed) {
        if is_error {
            eprintln!("{text}");
        } else {
            println!("{text}");
        }
    } else {
        let icon = if is_error {
            MB_ICONERROR
        } else {
            MB_ICONINFORMATION
        };
        message_box(text, MB_OK | icon);
    }
}

pub fn message_box(text: &str, style: MESSAGEBOX_STYLE) -> i32 {
    let (text, caption) = (wide(text), wide(APP_NAME));
    unsafe {
        MessageBoxW(
            null_mut(),
            text.as_ptr(),
            caption.as_ptr(),
            style | MB_SETFOREGROUND,
        )
    }
}

/// Yes/No question; `true` for Yes.
pub fn confirm(text: &str) -> bool {
    message_box(text, MB_YESNO | MB_ICONQUESTION) == IDYES
}
