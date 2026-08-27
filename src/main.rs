//! windows-resizer — macOS-style symmetric window resizing and a center-window hotkey
//! for Windows. Runs as a tiny background process whose only UI is a tray icon.
//!
//! Module layout:
//! * `geometry`, `keys`, `config` — platform-independent logic with unit tests.
//! * `icon` — the tray icon, rendered procedurally (also tested on any host).
//! * `ui`, `instance`, `window`, `hooks`, `tray`, `app` — the Win32 side (compiled on Windows only).

#![cfg_attr(windows, windows_subsystem = "windows")]
#![cfg_attr(not(windows), allow(dead_code))]

mod config;
mod geometry;
mod icon;
mod keys;

#[cfg(windows)]
#[macro_use]
mod ui;
#[cfg(windows)]
mod app;
#[cfg(windows)]
mod hooks;
#[cfg(windows)]
mod instance;
#[cfg(windows)]
mod tray;
#[cfg(windows)]
mod window;

#[cfg(windows)]
fn main() {
    std::process::exit(app::main());
}

#[cfg(not(windows))]
fn main() {
    // The binary itself is Windows-only; the pure modules still build (and are tested) here.
    match config::parse_args(std::env::args().skip(1)) {
        Ok(config::Command::Help) => print!("{}", config::HELP),
        Ok(config::Command::Version) => println!("windows-resizer {}", env!("CARGO_PKG_VERSION")),
        _ => {
            eprintln!("windows-resizer only runs on Windows; build it with --target x86_64-pc-windows-gnu.");
            std::process::exit(1);
        }
    }
}
