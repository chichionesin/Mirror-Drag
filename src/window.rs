//! Thin wrappers over the Win32 window APIs the tool needs.

use std::ffi::c_void;
use std::mem::{size_of, zeroed};
use std::ptr::null_mut;

use windows_sys::Win32::Foundation::{HWND, POINT, RECT};
use windows_sys::Win32::Graphics::Dwm::{DwmGetWindowAttribute, DWMWA_EXTENDED_FRAME_BOUNDS};
use windows_sys::Win32::Graphics::Gdi::{
    GetMonitorInfoW, MonitorFromWindow, MONITORINFO, MONITOR_DEFAULTTONEAREST,
};
use windows_sys::Win32::System::Threading::{AttachThreadInput, GetCurrentThreadId};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    GetAncestor, GetForegroundWindow, GetSystemMetrics, GetWindowRect, GetWindowThreadProcessId,
    IsZoomed, SendMessageTimeoutW, SetForegroundWindow, SetWindowPos, WindowFromPoint, GA_ROOT,
    HTBOTTOM, HTBOTTOMLEFT, HTBOTTOMRIGHT, HTLEFT, HTRIGHT, HTTOP, HTTOPLEFT, HTTOPRIGHT,
    MINMAXINFO, SET_WINDOW_POS_FLAGS, SMTO_ABORTIFHUNG, SM_CXMAXTRACK, SM_CXMINTRACK,
    SM_CYMAXTRACK, SM_CYMINTRACK, WM_GETMINMAXINFO, WM_NCHITTEST,
};

use crate::geometry::{Edges, Limits, Rect};

/// Upper bound for messages sent to other processes. A hung app must not stall the input hook:
/// Windows silently removes low-level hooks that take too long.
const SEND_TIMEOUT_MS: u32 = 100;

impl From<RECT> for Rect {
    fn from(r: RECT) -> Self {
        Self::new(r.left, r.top, r.right, r.bottom)
    }
}

fn root_of(hwnd: HWND) -> HWND {
    let root = unsafe { GetAncestor(hwnd, GA_ROOT) };
    if root.is_null() {
        hwnd
    } else {
        root
    }
}

/// Top-level window under a screen point, if any.
pub fn root_window_at(pt: POINT) -> Option<HWND> {
    let hwnd = unsafe { WindowFromPoint(pt) };
    (!hwnd.is_null()).then(|| root_of(hwnd))
}

/// Top-level window that currently has focus, if any.
pub fn foreground_window() -> Option<HWND> {
    let hwnd = unsafe { GetForegroundWindow() };
    (!hwnd.is_null()).then(|| root_of(hwnd))
}

fn send_with_timeout(hwnd: HWND, msg: u32, wparam: usize, lparam: isize) -> Option<usize> {
    let mut result = 0usize;
    let ok = unsafe {
        SendMessageTimeoutW(
            hwnd,
            msg,
            wparam,
            lparam,
            SMTO_ABORTIFHUNG,
            SEND_TIMEOUT_MS,
            &mut result,
        )
    };
    (ok != 0).then_some(result)
}

/// The resize edges a window reports for a screen point via `WM_NCHITTEST`, if any.
/// Works for custom frames too (browsers, Electron, UWP hosts) because the app answers itself.
pub fn resize_edges_at(hwnd: HWND, pt: POINT) -> Option<Edges> {
    let lparam = ((pt.x & 0xFFFF) | (pt.y << 16)) as isize;
    let hit = send_with_timeout(hwnd, WM_NCHITTEST, 0, lparam)? as u32;
    let edges = |left, top, right, bottom| Edges {
        left,
        top,
        right,
        bottom,
    };
    Some(match hit {
        HTLEFT => edges(true, false, false, false),
        HTRIGHT => edges(false, false, true, false),
        HTTOP => edges(false, true, false, false),
        HTBOTTOM => edges(false, false, false, true),
        HTTOPLEFT => edges(true, true, false, false),
        HTTOPRIGHT => edges(false, true, true, false),
        HTBOTTOMLEFT => edges(true, false, false, true),
        HTBOTTOMRIGHT => edges(false, false, true, true),
        _ => return None,
    })
}

/// Full window rectangle, including any invisible resize borders.
pub fn window_rect(hwnd: HWND) -> Option<Rect> {
    let mut rect: RECT = unsafe { zeroed() };
    (unsafe { GetWindowRect(hwnd, &mut rect) } != 0).then(|| rect.into())
}

/// What the user actually sees (DWM extended frame bounds, without invisible borders).
pub fn frame_bounds(hwnd: HWND) -> Option<Rect> {
    let mut rect: RECT = unsafe { zeroed() };
    let hr = unsafe {
        DwmGetWindowAttribute(
            hwnd,
            DWMWA_EXTENDED_FRAME_BOUNDS as u32,
            &mut rect as *mut RECT as *mut c_void,
            size_of::<RECT>() as u32,
        )
    };
    (hr == 0).then(|| rect.into())
}

/// Work area (screen minus taskbar) of the monitor the window is mostly on.
pub fn work_area(hwnd: HWND) -> Option<Rect> {
    let monitor = unsafe { MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST) };
    if monitor.is_null() {
        return None;
    }
    let mut info: MONITORINFO = unsafe { zeroed() };
    info.cbSize = size_of::<MONITORINFO>() as u32;
    (unsafe { GetMonitorInfoW(monitor, &mut info) } != 0).then(|| info.rcWork.into())
}

pub fn is_maximized(hwnd: HWND) -> bool {
    unsafe { IsZoomed(hwnd) != 0 }
}

/// Min/max tracking sizes the window asks for via `WM_GETMINMAXINFO`, seeded with the
/// system defaults exactly like `DefWindowProc` does. (The system marshals this message
/// across processes.) Falls back to the defaults if the window does not answer in time.
pub fn track_limits(hwnd: HWND) -> Limits {
    let metric = |index| unsafe { GetSystemMetrics(index) };
    let mut info: MINMAXINFO = unsafe { zeroed() };
    info.ptMinTrackSize = POINT {
        x: metric(SM_CXMINTRACK),
        y: metric(SM_CYMINTRACK),
    };
    info.ptMaxTrackSize = POINT {
        x: metric(SM_CXMAXTRACK),
        y: metric(SM_CYMAXTRACK),
    };
    send_with_timeout(
        hwnd,
        WM_GETMINMAXINFO,
        0,
        &mut info as *mut MINMAXINFO as isize,
    );
    let or_unlimited = |value: i32| {
        if value > 0 {
            value
        } else {
            Limits::default().max_width
        }
    };
    Limits {
        min_width: info.ptMinTrackSize.x.max(0),
        min_height: info.ptMinTrackSize.y.max(0),
        max_width: or_unlimited(info.ptMaxTrackSize.x),
        max_height: or_unlimited(info.ptMaxTrackSize.y),
    }
}

/// Moves/resizes the window; `flags` are `SWP_*` bits.
pub fn set_rect(hwnd: HWND, rect: Rect, flags: SET_WINDOW_POS_FLAGS) -> bool {
    unsafe {
        SetWindowPos(
            hwnd,
            null_mut(),
            rect.left,
            rect.top,
            rect.width(),
            rect.height(),
            flags,
        ) != 0
    }
}

/// Brings the window to the foreground. A background process is normally denied focus
/// changes, so input is temporarily attached to the current foreground thread (the classic
/// workaround); failure is harmless, the resize works either way.
pub fn activate(hwnd: HWND) {
    unsafe {
        let foreground = GetForegroundWindow();
        if foreground == hwnd {
            return;
        }
        let me = GetCurrentThreadId();
        let owner = if foreground.is_null() {
            0
        } else {
            GetWindowThreadProcessId(foreground, null_mut())
        };
        let attached = owner != 0 && owner != me && AttachThreadInput(me, owner, 1) != 0;
        SetForegroundWindow(hwnd);
        if attached {
            AttachThreadInput(me, owner, 0);
        }
    }
}
