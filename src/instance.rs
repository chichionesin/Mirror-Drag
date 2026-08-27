//! Single-instance guard and the cross-process "quit" signal.

use std::ptr::null;

use windows_sys::Win32::Foundation::{CloseHandle, GetLastError, ERROR_ALREADY_EXISTS, HANDLE};
use windows_sys::Win32::System::Threading::{
    CreateEventW, CreateMutexW, OpenEventW, SetEvent, EVENT_MODIFY_STATE,
};

use crate::ui::{last_error, wide};

// `Local\` scopes the objects to the current logon session.
const MUTEX_NAME: &str = "Local\\MirrorDrag.Instance";
const QUIT_EVENT_NAME: &str = "Local\\MirrorDrag.Quit";

/// Held by the primary instance for its whole lifetime.
pub struct Instance {
    mutex: HANDLE,
    quit_event: HANDLE,
}

impl Instance {
    /// Manual-reset event another process sets to ask this instance to exit.
    pub fn quit_event(&self) -> HANDLE {
        self.quit_event
    }
}

impl Drop for Instance {
    fn drop(&mut self) {
        unsafe {
            CloseHandle(self.quit_event);
            CloseHandle(self.mutex);
        }
    }
}

pub enum Acquire {
    Primary(Instance),
    AlreadyRunning,
}

pub fn acquire() -> Result<Acquire, String> {
    let mutex_name = wide(MUTEX_NAME);
    let mutex = unsafe { CreateMutexW(null(), 0, mutex_name.as_ptr()) };
    // Must be read before any other call can clobber it.
    let already_running = unsafe { GetLastError() } == ERROR_ALREADY_EXISTS;
    if mutex.is_null() {
        return Err(format!("CreateMutexW failed (error {})", last_error()));
    }
    if already_running {
        unsafe { CloseHandle(mutex) };
        return Ok(Acquire::AlreadyRunning);
    }

    let event_name = wide(QUIT_EVENT_NAME);
    let quit_event = unsafe { CreateEventW(null(), 1, 0, event_name.as_ptr()) };
    if quit_event.is_null() {
        let error = last_error();
        unsafe { CloseHandle(mutex) };
        return Err(format!("CreateEventW failed (error {error})"));
    }
    Ok(Acquire::Primary(Instance { mutex, quit_event }))
}

/// Asks the running instance to exit. Returns `false` when none is running.
pub fn signal_quit() -> bool {
    let event_name = wide(QUIT_EVENT_NAME);
    let event = unsafe { OpenEventW(EVENT_MODIFY_STATE, 0, event_name.as_ptr()) };
    if event.is_null() {
        return false;
    }
    let ok = unsafe { SetEvent(event) } != 0;
    unsafe { CloseHandle(event) };
    ok
}
