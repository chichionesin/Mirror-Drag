# Mirror-Drag

macOS has two small window tricks that Windows lacks:

* **Option + drag a window edge** resizes the window symmetrically — the opposite edge
  moves by the same amount and the window stays centered on the same point.
* **Window → Move & Resize → Center** puts the window exactly in the middle of the screen.

Mirror-Drag brings both to Windows as a single ~300 KB background executable —
just a tray icon, no installer, no runtime dependencies.

| Gesture                               | Effect                                                   |
|---------------------------------------|----------------------------------------------------------|
| **Alt + drag** any window edge/corner | Symmetric resize around the window center                |
| **Win + Alt + C**                     | Center the active window on its monitor's work area      |
| **Shake the mouse**                   | The pointer grows for a moment so you can find it        |

Both keys are configurable (see below). Every feature can be switched off from the tray menu,
which also has a *Start with Windows* toggle.

## Download

Grab `mirror-drag.exe` from the
[latest release](https://github.com/chichionesin/Mirror-Drag/releases/latest) (Windows 10/11,
x64). Each release ships a `.sha256` file with the checksum. The binary is unsigned, so
SmartScreen may show a warning on first run — it is built by the
[release workflow](.github/workflows/release.yml) on GitHub's own runners straight from the
tagged source, or you can [build it yourself](#building).

## Usage

Double-click `mirror-drag.exe`. It keeps running in the background and shows a small blue
icon in the notification area (tray); hover it to see the configured keys. Click the icon
for the menu:

* **Symmetric resize**, **Center window**, **Shake to find cursor** — check/uncheck to switch
  each feature on or off. The choice is remembered (`HKCU\Software\Mirror-Drag`).
* **Start with Windows** — adds/removes a Run entry that launches the exe (with the same
  command-line options) at logon.
* **Exit**.

Alternatively run the exe again — it asks whether to stop the running instance — or run
`mirror-drag.exe --quit` from a terminal.

```
mirror-drag.exe [OPTIONS]

  -m, --modifier <KEY>        Key to hold while dragging an edge: alt, ctrl, shift, win
                              [default: alt]
  -c, --center-hotkey <KEYS>  Hotkey that centers the active window, e.g. win+alt+c or
                              ctrl+shift+f9; 'none' disables it  [default: win+alt+c]
      --console               Open a console window and print debug output
  -q, --quit                  Stop the running instance
  -h, --help                  Show this help
  -V, --version               Show the version
```

Examples:

```
mirror-drag.exe                                  # Alt+drag, Win+Alt+C
mirror-drag.exe --modifier win --center-hotkey ctrl+alt+space
mirror-drag.exe --center-hotkey none             # symmetric resize only
mirror-drag.exe --console                        # watch what it does
```

## Notes and limitations

* **Elevated windows.** Windows does not let a normal process touch windows of programs
  running as administrator (UIPI). Alt+drag on such a window falls back to the normal
  resize. Run `mirror-drag.exe` as administrator if you need it there too.
* **Alt and menus.** Releasing Alt on its own focuses an application's menu bar. After a
  drag the tool taps Ctrl before the Alt release reaches the app (the same trick AltDrag /
  AltSnap use), so menus stay closed. The same applies to the Win key and the Start menu.
* **Alt+drag is used by some apps** (3D viewports, games). Pick another modifier with
  `--modifier` if that gets in the way.
* **Size limits.** Windows that enforce minimum/maximum sizes stop growing or shrinking at
  those limits but stay centered.
* **Alt + Shift** is the default keyboard-layout switch on Windows, so avoid `alt+shift+…`
  as the center hotkey.
* Maximized windows are ignored by the center hotkey; unmaximize first.
* **Shake to find** uses the pointer-size setting of Windows 10 1809+ (*Settings → Accessibility →
  Mouse pointer*). The size is bumped for ~1 s and put back; if the process is killed in that
  moment, the next start restores it. It is a system-wide setting, so the enlarged pointer
  briefly shows in every app — that is the point.

## Building

Rust 1.85+ is required.

On Windows:

```
cargo build --release
```

Cross-compiling from macOS (or Linux) needs the MinGW-w64 linker:

```
brew install mingw-w64                      # Debian/Ubuntu: apt install gcc-mingw-w64-x86-64
rustup target add x86_64-pc-windows-gnu
cargo build --release --target x86_64-pc-windows-gnu
# -> target/x86_64-pc-windows-gnu/release/mirror-drag.exe
```

`.cargo/config.toml` already points the GNU target at `x86_64-w64-mingw32-gcc`.

The platform-independent parts (geometry, hotkey parsing, CLI) are unit-tested and run on
any host:

```
cargo test
cargo clippy --all-targets -- -D warnings
cargo clippy --target x86_64-pc-windows-gnu --all-targets -- -D warnings
```

## How it works

* A low-level mouse hook (`WH_MOUSE_LL`) sees every click before the target application.
  If the modifier is held, the window under the cursor is asked where the click landed
  (`WM_NCHITTEST`). When it reports a resize edge or corner, the click is swallowed and the
  tool performs the resize itself with `SetWindowPos`, mirroring the dragged edge onto the
  opposite one. Because the app answers the hit test, this also works for custom frames
  (browsers, Electron apps, UWP hosts). `WM_GETMINMAXINFO` provides the size limits.
* A low-level keyboard hook (`WH_KEYBOARD_LL`) only watches for the modifier's release to
  suppress the menu-bar / Start-menu activation described above.
* `RegisterHotKey` provides the center hotkey. The window's visible bounds (DWM extended
  frame, without the invisible resize borders) are centered on the monitor's work area, so
  the result looks centered rather than being centered on paper.
* The process is per-monitor DPI aware (v2), so all coordinates are physical pixels and
  mixed-DPI setups work.
* Shake detection counts direction reversals of the pointer: four strokes of ≥ 30 px within
  600 ms on either axis. The hook only posts a thread message; the message loop then writes
  `CursorBaseSize` and calls `SystemParametersInfo(SPI_SETCURSORS)`, and a thread timer
  restores the original size.
* The tray icon is a `Shell_NotifyIcon` owned by a hidden window; its context menu toggles
  the features (persisted in the registry), the Run entry, and *Exit* posts `WM_QUIT`. The icon itself is rendered at startup from a few rectangles (no image
  resources), and is re-added automatically when Explorer restarts (`TaskbarCreated`).
* A named mutex enforces a single instance; a named event lets `--quit` (or a second launch)
  stop it cleanly, unhooking everything on the way out.

Everything is plain Win32 through the [`windows-sys`](https://crates.io/crates/windows-sys)
bindings — no other dependencies.

## License

MIT — see [LICENSE](LICENSE).
