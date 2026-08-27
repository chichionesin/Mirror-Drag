//! Notification-area (tray) icon with a context menu, hosted by a hidden window.

use std::cell::RefCell;
use std::mem::{size_of, zeroed};
use std::ptr::{null, null_mut};

use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::Shell::{
    Shell_NotifyIconW, NIF_ICON, NIF_MESSAGE, NIF_SHOWTIP, NIF_TIP, NIM_ADD, NIM_DELETE,
    NIM_SETVERSION, NIN_SELECT, NOTIFYICONDATAW, NOTIFYICON_VERSION_4,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CreatePopupMenu, CreateWindowExW, DefWindowProcW, DestroyIcon, DestroyMenu,
    DestroyWindow, PostMessageW, PostQuitMessage, RegisterClassW, RegisterWindowMessageW,
    SetForegroundWindow, TrackPopupMenu, UnregisterClassW, HICON, MF_GRAYED, MF_SEPARATOR,
    MF_STRING, TPM_BOTTOMALIGN, TPM_LEFTALIGN, TPM_RETURNCMD, TPM_RIGHTBUTTON, WM_APP,
    WM_CONTEXTMENU, WM_NULL, WNDCLASSW, WS_OVERLAPPED,
};

use crate::config::Config;
use crate::icon;
use crate::ui::{last_error, wide, APP_NAME};

const CLASS_NAME: &str = "MirrorDragTray";
const ICON_ID: u32 = 1;
/// Callback message the shell sends for icon events.
const WM_TRAY: u32 = WM_APP + 1;
/// `NIN_SELECT | NINF_KEY`: the icon was selected with the keyboard.
const NIN_KEYSELECT: u32 = NIN_SELECT | 1;
const MENU_EXIT: usize = 1;

struct State {
    data: NOTIFYICONDATAW,
    /// Informational (disabled) lines shown at the top of the menu.
    info_lines: Vec<String>,
    /// Broadcast by Explorer when it (re)starts; the icon must then be added again.
    taskbar_created: u32,
}

thread_local! {
    static STATE: RefCell<Option<State>> = const { RefCell::new(None) };
}

/// RAII handle: dropping it removes the icon and destroys the hidden window.
pub struct Tray {
    hwnd: HWND,
    icon: HICON,
}

pub fn install(config: &Config) -> Result<Tray, String> {
    let module = unsafe { GetModuleHandleW(null()) };
    let class_name = wide(CLASS_NAME);
    let class = WNDCLASSW {
        lpfnWndProc: Some(wnd_proc),
        hInstance: module,
        lpszClassName: class_name.as_ptr(),
        ..unsafe { zeroed() }
    };
    if unsafe { RegisterClassW(&class) } == 0 {
        return Err(format!("RegisterClassW failed (error {})", last_error()));
    }
    let title = wide(APP_NAME);
    let hwnd = unsafe {
        CreateWindowExW(
            0,
            class_name.as_ptr(),
            title.as_ptr(),
            WS_OVERLAPPED,
            0,
            0,
            0,
            0,
            null_mut(),
            null_mut(),
            module,
            null(),
        )
    };
    if hwnd.is_null() {
        let error = last_error();
        unsafe { UnregisterClassW(class_name.as_ptr(), module) };
        return Err(format!("CreateWindowExW failed (error {error})"));
    }
    let tray = Tray {
        hwnd,
        icon: icon::create_icon(),
    };

    let mut data: NOTIFYICONDATAW = unsafe { zeroed() };
    data.cbSize = size_of::<NOTIFYICONDATAW>() as u32;
    data.hWnd = hwnd;
    data.uID = ICON_ID;
    data.uFlags = NIF_MESSAGE | NIF_ICON | NIF_TIP | NIF_SHOWTIP;
    data.uCallbackMessage = WM_TRAY;
    data.hIcon = tray.icon;
    data.Anonymous.uVersion = NOTIFYICON_VERSION_4;
    copy_tip(&mut data.szTip, &tooltip(config));
    let taskbar_created = unsafe { RegisterWindowMessageW(wide("TaskbarCreated").as_ptr()) };
    STATE.with(|state| {
        *state.borrow_mut() = Some(State {
            data,
            info_lines: info_lines(config),
            taskbar_created,
        });
    });

    if !add_icon() {
        return Err(format!("Shell_NotifyIconW failed (error {})", last_error()));
    }
    Ok(tray)
}

impl Drop for Tray {
    fn drop(&mut self) {
        STATE.with(|state| {
            if let Some(state) = state.borrow_mut().take() {
                unsafe { Shell_NotifyIconW(NIM_DELETE, &state.data) };
            }
        });
        unsafe {
            DestroyWindow(self.hwnd);
            UnregisterClassW(wide(CLASS_NAME).as_ptr(), GetModuleHandleW(null()));
            if !self.icon.is_null() {
                DestroyIcon(self.icon);
            }
        }
    }
}

fn add_icon() -> bool {
    STATE.with(|state| {
        let state = state.borrow();
        let Some(state) = state.as_ref() else {
            return false;
        };
        unsafe {
            Shell_NotifyIconW(NIM_ADD, &state.data) != 0
                && Shell_NotifyIconW(NIM_SETVERSION, &state.data) != 0
        }
    })
}

fn taskbar_created_message() -> Option<u32> {
    STATE.with(|state| {
        state
            .borrow()
            .as_ref()
            .map(|state| state.taskbar_created)
            .filter(|&msg| msg != 0)
    })
}

unsafe extern "system" fn wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_TRAY => {
            // NOTIFYICON_VERSION_4: the event is in LOWORD(lParam), the anchor point in wParam.
            let event = (lparam as u32) & 0xFFFF;
            if matches!(event, WM_CONTEXTMENU | NIN_SELECT | NIN_KEYSELECT) {
                let x = (wparam as u32 & 0xFFFF) as i16 as i32;
                let y = ((wparam as u32 >> 16) & 0xFFFF) as i16 as i32;
                show_menu(hwnd, x, y);
            }
            0
        }
        _ if Some(msg) == taskbar_created_message() => {
            log!("Explorer restarted, re-adding the tray icon");
            add_icon();
            0
        }
        _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
    }
}

fn show_menu(hwnd: HWND, x: i32, y: i32) {
    let lines = STATE.with(|state| {
        state
            .borrow()
            .as_ref()
            .map(|state| state.info_lines.clone())
            .unwrap_or_default()
    });
    unsafe {
        let menu = CreatePopupMenu();
        if menu.is_null() {
            return;
        }
        for line in &lines {
            let text = wide(line);
            AppendMenuW(menu, MF_STRING | MF_GRAYED, 0, text.as_ptr());
        }
        AppendMenuW(menu, MF_SEPARATOR, 0, null());
        let exit = wide("Exit");
        AppendMenuW(menu, MF_STRING, MENU_EXIT, exit.as_ptr());

        // Without this the menu would not close when the user clicks elsewhere (KB 135788).
        SetForegroundWindow(hwnd);
        let flags = TPM_RETURNCMD | TPM_RIGHTBUTTON | TPM_LEFTALIGN | TPM_BOTTOMALIGN;
        let choice = TrackPopupMenu(menu, flags, x, y, 0, hwnd, null());
        PostMessageW(hwnd, WM_NULL, 0, 0);
        DestroyMenu(menu);

        if choice as usize == MENU_EXIT {
            log!("exit chosen from the tray menu");
            PostQuitMessage(0);
        }
    }
}

fn resize_hint(config: &Config) -> String {
    format!(
        "{} + drag a window edge: symmetric resize",
        config.modifier.name()
    )
}

fn center_hint(config: &Config) -> String {
    match config.center_hotkey {
        Some(hotkey) => format!("{}: center the active window", hotkey.describe()),
        None => "Center hotkey: disabled".to_string(),
    }
}

fn info_lines(config: &Config) -> Vec<String> {
    vec![
        concat!("Mirror-Drag ", env!("CARGO_PKG_VERSION")).to_string(),
        resize_hint(config),
        center_hint(config),
    ]
}

fn tooltip(config: &Config) -> String {
    format!(
        "{APP_NAME}\n{}\n{}",
        resize_hint(config),
        center_hint(config)
    )
}

/// Copies `text` into a fixed NUL-terminated UTF-16 buffer, truncating if needed.
fn copy_tip(dst: &mut [u16; 128], text: &str) {
    let src: Vec<u16> = text.encode_utf16().take(dst.len() - 1).collect();
    dst[..src.len()].copy_from_slice(&src);
    dst[src.len()] = 0;
}
