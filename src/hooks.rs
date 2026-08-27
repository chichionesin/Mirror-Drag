//! Low-level mouse and keyboard hooks implementing the "modifier + drag an edge" gesture.
//!
//! Both hooks run on the thread that installed them (the main thread's message loop), so the
//! drag state lives in a thread-local. A hook can be re-entered while it waits on another
//! process, hence `try_borrow_mut` everywhere: a nested call simply passes the event through.

use std::cell::RefCell;
use std::mem::size_of;
use std::ptr::{null, null_mut};

use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, WPARAM};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP,
    VK_CONTROL, VK_LCONTROL, VK_LMENU, VK_LSHIFT, VK_LWIN, VK_MENU, VK_RCONTROL, VK_RMENU,
    VK_RSHIFT, VK_RWIN, VK_SHIFT,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, SetWindowsHookExW, UnhookWindowsHookEx, HC_ACTION, HHOOK, KBDLLHOOKSTRUCT,
    LLKHF_INJECTED, LLMHF_INJECTED, MSLLHOOKSTRUCT, SWP_ASYNCWINDOWPOS, SWP_NOACTIVATE,
    SWP_NOZORDER, WH_KEYBOARD_LL, WH_MOUSE_LL, WM_KEYUP, WM_LBUTTONDOWN, WM_LBUTTONUP,
    WM_MOUSEMOVE, WM_SYSKEYUP,
};

use crate::geometry::{symmetric_resize, Edges, Limits, Rect};
use crate::keys::Modifier;
use crate::settings::{self, Feature};
use crate::shake::ShakeDetector;
use crate::ui::last_error;
use crate::{cursor, window};

/// An in-progress symmetric resize.
struct Drag {
    hwnd: HWND,
    edges: Edges,
    origin: POINT,
    start: Rect,
    last: Rect,
    limits: Limits,
}

struct State {
    modifier: Modifier,
    /// Set once the modifier has been used for a drag: its release must then not open a menu.
    modifier_used: bool,
    drag: Option<Drag>,
    shake: ShakeDetector,
}

thread_local! {
    static STATE: RefCell<State> = const {
        RefCell::new(State {
            modifier: Modifier::Alt,
            modifier_used: false,
            drag: None,
            shake: ShakeDetector::new(),
        })
    };
}

/// RAII handle; dropping it removes both hooks.
pub struct Hooks {
    mouse: HHOOK,
    keyboard: HHOOK,
}

impl Drop for Hooks {
    fn drop(&mut self) {
        unsafe {
            UnhookWindowsHookEx(self.keyboard);
            UnhookWindowsHookEx(self.mouse);
        }
    }
}

/// Installs the hooks on the calling thread, which must run a message loop.
pub fn install(modifier: Modifier) -> Result<Hooks, String> {
    STATE.with(|state| state.borrow_mut().modifier = modifier);
    let module = unsafe { GetModuleHandleW(null()) };
    let mouse = unsafe { SetWindowsHookExW(WH_MOUSE_LL, Some(mouse_proc), module, 0) };
    if mouse.is_null() {
        return Err(format!(
            "failed to install the mouse hook (error {})",
            last_error()
        ));
    }
    let keyboard = unsafe { SetWindowsHookExW(WH_KEYBOARD_LL, Some(keyboard_proc), module, 0) };
    if keyboard.is_null() {
        let error = last_error();
        unsafe { UnhookWindowsHookEx(mouse) };
        return Err(format!(
            "failed to install the keyboard hook (error {error})"
        ));
    }
    Ok(Hooks { mouse, keyboard })
}

unsafe extern "system" fn mouse_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code == HC_ACTION as i32 {
        // SAFETY: for HC_ACTION the system passes a valid MSLLHOOKSTRUCT in lparam.
        let info = unsafe { &*(lparam as *const MSLLHOOKSTRUCT) };
        if info.flags & LLMHF_INJECTED == 0 {
            let swallow = match wparam as u32 {
                WM_LBUTTONDOWN => on_button_down(info.pt),
                WM_MOUSEMOVE => on_mouse_move(info.pt, info.time),
                WM_LBUTTONUP => on_button_up(),
                _ => false,
            };
            if swallow {
                return 1;
            }
        }
    }
    unsafe { CallNextHookEx(null_mut(), code, wparam, lparam) }
}

unsafe extern "system" fn keyboard_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code == HC_ACTION as i32 {
        // SAFETY: for HC_ACTION the system passes a valid KBDLLHOOKSTRUCT in lparam.
        let info = unsafe { &*(lparam as *const KBDLLHOOKSTRUCT) };
        let message = wparam as u32;
        if info.flags & LLKHF_INJECTED == 0 && (message == WM_KEYUP || message == WM_SYSKEYUP) {
            on_key_up(info.vkCode);
        }
    }
    unsafe { CallNextHookEx(null_mut(), code, wparam, lparam) }
}

/// Starts a symmetric resize when the modifier is held and the click lands on a resize edge.
/// Returns `true` if the click was consumed (the app must not start its own resize).
fn on_button_down(pt: POINT) -> bool {
    STATE.with(|state| {
        let Ok(mut state) = state.try_borrow_mut() else {
            return false;
        };
        if state.drag.take().is_some() {
            log!("discarding stale drag");
            return false;
        }
        if !settings::is_enabled(Feature::SymmetricResize) || !modifier_is_down(state.modifier) {
            return false;
        }
        let Some(hwnd) = window::root_window_at(pt) else {
            return false;
        };
        let Some(edges) = window::resize_edges_at(hwnd, pt) else {
            return false;
        };
        let Some(start) = window::window_rect(hwnd) else {
            return false;
        };
        let limits = window::track_limits(hwnd);
        window::activate(hwnd);
        log!("drag start: hwnd={hwnd:?} edges={edges:?} rect={start:?} limits={limits:?}");
        state.drag = Some(Drag {
            hwnd,
            edges,
            origin: pt,
            start,
            last: start,
            limits,
        });
        state.modifier_used = true;
        true
    })
}

/// Moves are never swallowed, so the cursor keeps tracking the edge it holds.
fn on_mouse_move(pt: POINT, time: u32) -> bool {
    STATE.with(|state| {
        let Ok(mut state) = state.try_borrow_mut() else {
            return false;
        };
        let Some(drag) = state.drag.as_mut() else {
            // Shake detection only runs outside a drag; the actual pointer change happens on the
            // message loop so this hook never waits on other windows.
            if settings::is_enabled(Feature::ShakeToFind) && state.shake.feed(pt.x, pt.y, time) {
                cursor::request();
            }
            return false;
        };
        let (dx, dy) = (pt.x - drag.origin.x, pt.y - drag.origin.y);
        let target = symmetric_resize(drag.start, drag.edges, dx, dy, &drag.limits);
        if target != drag.last {
            drag.last = target;
            // Async: never block the hook on a busy target process.
            window::set_rect(
                drag.hwnd,
                target,
                SWP_NOZORDER | SWP_NOACTIVATE | SWP_ASYNCWINDOWPOS,
            );
        }
        false
    })
}

fn on_button_up() -> bool {
    STATE.with(|state| {
        let Ok(mut state) = state.try_borrow_mut() else {
            return false;
        };
        match state.drag.take() {
            Some(drag) => {
                log!("drag end: rect={:?}", drag.last);
                true
            }
            None => false,
        }
    })
}

/// Releasing Alt or Win on its own activates the menu bar / Start menu. After a drag we tap
/// Ctrl first (the AltDrag/AltSnap trick) so the release is no longer "on its own".
fn on_key_up(vk: u32) {
    STATE.with(|state| {
        let Ok(mut state) = state.try_borrow_mut() else {
            return;
        };
        if !state.modifier_used || !is_modifier_key(state.modifier, vk) {
            return;
        }
        state.modifier_used = false;
        if matches!(state.modifier, Modifier::Alt | Modifier::Win) {
            tap_ctrl();
        }
    });
}

fn modifier_is_down(modifier: Modifier) -> bool {
    let down = |vk: u16| (unsafe { GetAsyncKeyState(i32::from(vk)) } as u16) & 0x8000 != 0;
    match modifier {
        Modifier::Alt => down(VK_MENU),
        Modifier::Ctrl => down(VK_CONTROL),
        Modifier::Shift => down(VK_SHIFT),
        Modifier::Win => down(VK_LWIN) || down(VK_RWIN),
    }
}

fn is_modifier_key(modifier: Modifier, vk: u32) -> bool {
    let keys = match modifier {
        Modifier::Alt => [VK_LMENU, VK_RMENU],
        Modifier::Ctrl => [VK_LCONTROL, VK_RCONTROL],
        Modifier::Shift => [VK_LSHIFT, VK_RSHIFT],
        Modifier::Win => [VK_LWIN, VK_RWIN],
    };
    keys.iter().any(|key| u32::from(*key) == vk)
}

fn tap_ctrl() {
    let key = |flags| INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: VK_CONTROL,
                wScan: 0,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: 0,
            },
        },
    };
    let inputs = [key(0), key(KEYEVENTF_KEYUP)];
    unsafe {
        SendInput(
            inputs.len() as u32,
            inputs.as_ptr(),
            size_of::<INPUT>() as i32,
        )
    };
}
