//! Process entry point: argument handling, single-instance guard, hooks, hotkey, message loop.

use std::mem::zeroed;
use std::ptr::null_mut;

use windows_sys::Win32::Foundation::{HANDLE, WAIT_OBJECT_0};
use windows_sys::Win32::System::Threading::INFINITE;
use windows_sys::Win32::UI::HiDpi::{
    SetProcessDpiAwarenessContext, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    RegisterHotKey, UnregisterHotKey, MOD_NOREPEAT,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    DispatchMessageW, MsgWaitForMultipleObjects, PeekMessageW, SetProcessDPIAware,
    TranslateMessage, MB_ICONERROR, MSG, PM_REMOVE, QS_ALLINPUT, SWP_NOACTIVATE, SWP_NOSIZE,
    SWP_NOZORDER, WM_HOTKEY, WM_QUIT,
};

use crate::config::{self, Command, Config};
use crate::geometry::center_in;
use crate::instance::{self, Acquire};
use crate::keys::Hotkey;
use crate::{hooks, tray, ui, window};

const CENTER_HOTKEY_ID: i32 = 1;

pub fn main() -> i32 {
    // With the "windows" subsystem a panic would otherwise vanish silently.
    std::panic::set_hook(Box::new(|info| {
        ui::message_box(&format!("Unexpected error: {info}"), MB_ICONERROR);
    }));

    let command = match config::parse_args(std::env::args().skip(1)) {
        Ok(command) => command,
        Err(error) => {
            ui::attach_parent_console();
            ui::report(&format!("error: {error}\n\n{}", config::HELP), true);
            return 2;
        }
    };
    match command {
        Command::Help => {
            ui::attach_parent_console();
            ui::report(config::HELP, false);
            0
        }
        Command::Version => {
            ui::attach_parent_console();
            ui::report(
                concat!("windows-resizer ", env!("CARGO_PKG_VERSION")),
                false,
            );
            0
        }
        Command::Quit => {
            ui::attach_parent_console();
            if instance::signal_quit() {
                0
            } else {
                ui::report("windows-resizer is not running.", true);
                1
            }
        }
        Command::Run(config) => run(&config),
    }
}

fn run(config: &Config) -> i32 {
    if config.console {
        ui::open_console();
    }
    let instance = match instance::acquire() {
        Ok(Acquire::Primary(instance)) => instance,
        Ok(Acquire::AlreadyRunning) => {
            if ui::confirm(&format!("{} is already running.\n\nStop it?", ui::APP_NAME)) {
                instance::signal_quit();
            }
            return 0;
        }
        Err(error) => {
            ui::report(&error, true);
            return 1;
        }
    };

    enable_dpi_awareness();
    let _hooks = match hooks::install(config.modifier) {
        Ok(hooks) => hooks,
        Err(error) => {
            ui::report(&error, true);
            return 1;
        }
    };
    let _hotkey = config.center_hotkey.and_then(register_center_hotkey);
    // Not fatal: without the icon the tool still works and --quit still stops it.
    let _tray = tray::install(config)
        .map_err(|error| ui::report(&format!("Tray icon unavailable: {error}"), true))
        .ok();
    log!(
        "running: modifier={} center-hotkey={}",
        config.modifier.name(),
        config
            .center_hotkey
            .map_or_else(|| "none".to_string(), |hotkey| hotkey.describe())
    );

    message_loop(instance.quit_event());
    log!("exiting");
    0
}

/// Physical-pixel coordinates everywhere, matching the low-level hooks and other windows.
fn enable_dpi_awareness() {
    if unsafe { SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) } == 0 {
        // Windows 10 before 1703.
        unsafe { SetProcessDPIAware() };
    }
}

struct HotkeyGuard(i32);

impl Drop for HotkeyGuard {
    fn drop(&mut self) {
        unsafe { UnregisterHotKey(null_mut(), self.0) };
    }
}

/// A failure here is not fatal: the resize gesture still works, so just tell the user.
fn register_center_hotkey(hotkey: Hotkey) -> Option<HotkeyGuard> {
    let ok = unsafe {
        RegisterHotKey(
            null_mut(),
            CENTER_HOTKEY_ID,
            hotkey.modifiers | MOD_NOREPEAT,
            hotkey.vk,
        )
    } != 0;
    if ok {
        return Some(HotkeyGuard(CENTER_HOTKEY_ID));
    }
    ui::report(
        &format!(
            "Could not register the center-window hotkey {} (error {}); another program probably \
             uses it.\n\nSymmetric resizing still works. Choose another key with --center-hotkey.",
            hotkey.describe(),
            ui::last_error()
        ),
        true,
    );
    None
}

/// Pumps messages (required for the hooks and WM_HOTKEY) until the quit event is signaled.
fn message_loop(quit_event: HANDLE) {
    let mut msg: MSG = unsafe { zeroed() };
    loop {
        let wake = unsafe { MsgWaitForMultipleObjects(1, &quit_event, 0, INFINITE, QS_ALLINPUT) };
        if wake == WAIT_OBJECT_0 {
            log!("quit requested by another instance");
            return;
        }
        if wake != WAIT_OBJECT_0 + 1 {
            log!(
                "MsgWaitForMultipleObjects failed (error {})",
                ui::last_error()
            );
            return;
        }
        while unsafe { PeekMessageW(&mut msg, null_mut(), 0, 0, PM_REMOVE) } != 0 {
            match msg.message {
                WM_QUIT => return,
                WM_HOTKEY if msg.wParam == CENTER_HOTKEY_ID as usize => center_active_window(),
                _ => unsafe {
                    TranslateMessage(&msg);
                    DispatchMessageW(&msg);
                },
            }
        }
    }
}

/// Centers the foreground window on the work area of its monitor.
fn center_active_window() {
    let Some(hwnd) = window::foreground_window() else {
        log!("center: no foreground window");
        return;
    };
    if window::is_maximized(hwnd) {
        log!("center: window {hwnd:?} is maximized, skipping");
        return;
    }
    let (Some(rect), Some(area)) = (window::window_rect(hwnd), window::work_area(hwnd)) else {
        log!("center: cannot query window {hwnd:?}");
        return;
    };
    // Center what the user sees (DWM frame) rather than the rect padded by invisible borders.
    let visible = window::frame_bounds(hwnd).unwrap_or(rect);
    let centered = center_in(visible, area);
    let target = rect.offset(centered.left - visible.left, centered.top - visible.top);
    let ok = window::set_rect(hwnd, target, SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE);
    log!("center: hwnd={hwnd:?} {rect:?} -> {target:?} ok={ok}");
}
