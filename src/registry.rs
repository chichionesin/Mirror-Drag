//! Minimal helpers for values under HKEY_CURRENT_USER.

use std::ptr::{null, null_mut};

use windows_sys::Win32::Foundation::ERROR_SUCCESS;
use windows_sys::Win32::System::Registry::{
    RegCloseKey, RegCreateKeyExW, RegDeleteValueW, RegOpenKeyExW, RegQueryValueExW, RegSetValueExW,
    HKEY, HKEY_CURRENT_USER, KEY_QUERY_VALUE, KEY_SET_VALUE, REG_DWORD, REG_OPTION_NON_VOLATILE,
    REG_SZ,
};

use crate::ui::wide;

struct Key(HKEY);

impl Drop for Key {
    fn drop(&mut self) {
        unsafe { RegCloseKey(self.0) };
    }
}

fn open(path: &str, access: u32) -> Option<Key> {
    let mut key: HKEY = null_mut();
    let path = wide(path);
    let status = unsafe { RegOpenKeyExW(HKEY_CURRENT_USER, path.as_ptr(), 0, access, &mut key) };
    (status == ERROR_SUCCESS).then_some(Key(key))
}

fn create(path: &str) -> Option<Key> {
    let mut key: HKEY = null_mut();
    let path = wide(path);
    let status = unsafe {
        RegCreateKeyExW(
            HKEY_CURRENT_USER,
            path.as_ptr(),
            0,
            null(),
            REG_OPTION_NON_VOLATILE,
            KEY_SET_VALUE | KEY_QUERY_VALUE,
            null(),
            &mut key,
            null_mut(),
        )
    };
    (status == ERROR_SUCCESS).then_some(Key(key))
}

pub fn read_dword(path: &str, name: &str) -> Option<u32> {
    let key = open(path, KEY_QUERY_VALUE)?;
    let name = wide(name);
    let (mut value, mut size, mut kind) = (0u32, 4u32, 0u32);
    let status = unsafe {
        RegQueryValueExW(
            key.0,
            name.as_ptr(),
            null(),
            &mut kind,
            &mut value as *mut u32 as *mut u8,
            &mut size,
        )
    };
    (status == ERROR_SUCCESS && kind == REG_DWORD && size == 4).then_some(value)
}

pub fn write_dword(path: &str, name: &str, value: u32) -> bool {
    let Some(key) = create(path) else {
        return false;
    };
    let name = wide(name);
    let status = unsafe {
        RegSetValueExW(
            key.0,
            name.as_ptr(),
            0,
            REG_DWORD,
            &value as *const u32 as *const u8,
            4,
        )
    };
    status == ERROR_SUCCESS
}

pub fn write_string(path: &str, name: &str, value: &str) -> bool {
    let Some(key) = create(path) else {
        return false;
    };
    let name = wide(name);
    let data = wide(value);
    let bytes = (data.len() * std::mem::size_of::<u16>()) as u32;
    let status = unsafe {
        RegSetValueExW(
            key.0,
            name.as_ptr(),
            0,
            REG_SZ,
            data.as_ptr() as *const u8,
            bytes,
        )
    };
    status == ERROR_SUCCESS
}

pub fn value_exists(path: &str, name: &str) -> bool {
    let Some(key) = open(path, KEY_QUERY_VALUE) else {
        return false;
    };
    let name = wide(name);
    let status = unsafe {
        RegQueryValueExW(
            key.0,
            name.as_ptr(),
            null(),
            null_mut(),
            null_mut(),
            null_mut(),
        )
    };
    status == ERROR_SUCCESS
}

pub fn delete_value(path: &str, name: &str) -> bool {
    let Some(key) = open(path, KEY_SET_VALUE) else {
        return false;
    };
    let name = wide(name);
    unsafe { RegDeleteValueW(key.0, name.as_ptr()) == ERROR_SUCCESS }
}
