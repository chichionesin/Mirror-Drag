//! Command-line configuration. Hand-rolled to keep the binary dependency-free.

use crate::keys::{Hotkey, Modifier};

pub const DEFAULT_MODIFIER: &str = "alt";
pub const DEFAULT_CENTER_HOTKEY: &str = "win+alt+c";

pub const HELP: &str = concat!(
    "mirror-drag ",
    env!("CARGO_PKG_VERSION"),
    "\n",
    "macOS-style window tricks for Windows: hold a modifier while dragging a window edge\n",
    "to resize it symmetrically around its center, plus a hotkey that centers the active window.\n",
    "\n",
    "USAGE:\n",
    "  mirror-drag.exe [OPTIONS]\n",
    "\n",
    "OPTIONS:\n",
    "  -m, --modifier <KEY>        Key to hold while dragging an edge: alt, ctrl, shift, win\n",
    "                              [default: alt]\n",
    "  -c, --center-hotkey <KEYS>  Hotkey that centers the active window, e.g. win+alt+c or\n",
    "                              ctrl+shift+f9; 'none' disables it  [default: win+alt+c]\n",
    "      --console               Open a console window and print debug output\n",
    "  -q, --quit                  Stop the running instance\n",
    "  -h, --help                  Show this help\n",
    "  -V, --version               Show the version\n",
    "\n",
    "The tool runs in the background with a tray icon: right-click it and choose Exit to\n",
    "stop. Launching it a second time asks whether to stop the running instance; --quit\n",
    "stops it without asking.\n",
);

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Config {
    pub modifier: Modifier,
    /// `None` disables the center hotkey.
    pub center_hotkey: Option<Hotkey>,
    pub console: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            modifier: Modifier::parse(DEFAULT_MODIFIER).expect("valid default modifier"),
            center_hotkey: Some(
                Hotkey::parse(DEFAULT_CENTER_HOTKEY).expect("valid default hotkey"),
            ),
            console: false,
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum Command {
    Run(Config),
    Quit,
    Help,
    Version,
}

/// Parses the process arguments (without the program name).
pub fn parse_args<I>(args: I) -> Result<Command, String>
where
    I: IntoIterator<Item = String>,
{
    let mut config = Config::default();
    let mut args = args.into_iter();
    while let Some(arg) = args.next() {
        // Accept both `--opt value` and `--opt=value`.
        let (name, inline) = match arg.split_once('=') {
            Some((name, value)) => (name.to_string(), Some(value.to_string())),
            None => (arg, None),
        };
        match name.as_str() {
            "-h" | "--help" => return Ok(Command::Help),
            "-V" | "--version" => return Ok(Command::Version),
            "-q" | "--quit" => return Ok(Command::Quit),
            "--console" => config.console = true,
            "-m" | "--modifier" => {
                config.modifier = Modifier::parse(&take_value(&name, inline, &mut args)?)?;
            }
            "-c" | "--center-hotkey" => {
                let value = take_value(&name, inline, &mut args)?;
                config.center_hotkey =
                    if value.eq_ignore_ascii_case("none") || value.eq_ignore_ascii_case("off") {
                        None
                    } else {
                        Some(Hotkey::parse(&value)?)
                    };
            }
            other => return Err(format!("unknown option '{other}'")),
        }
    }
    Ok(Command::Run(config))
}

fn take_value(
    name: &str,
    inline: Option<String>,
    args: &mut impl Iterator<Item = String>,
) -> Result<String, String> {
    inline
        .or_else(|| args.next())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| format!("option '{name}' requires a value"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::keys::{MOD_CONTROL, MOD_SHIFT};

    fn parse(args: &[&str]) -> Result<Command, String> {
        parse_args(args.iter().map(|s| s.to_string()))
    }

    #[test]
    fn no_args_gives_defaults() {
        assert_eq!(parse(&[]), Ok(Command::Run(Config::default())));
    }

    #[test]
    fn parses_options_in_both_styles() {
        let expected = Config {
            modifier: Modifier::Ctrl,
            center_hotkey: Some(Hotkey {
                modifiers: MOD_CONTROL | MOD_SHIFT,
                vk: 0x78,
            }),
            console: true,
        };
        assert_eq!(
            parse(&[
                "--modifier",
                "ctrl",
                "--center-hotkey=ctrl+shift+f9",
                "--console"
            ]),
            Ok(Command::Run(expected.clone()))
        );
        assert_eq!(
            parse(&["-m=ctrl", "-c", "Ctrl+Shift+F9", "--console"]),
            Ok(Command::Run(expected))
        );
    }

    #[test]
    fn hotkey_can_be_disabled() {
        match parse(&["--center-hotkey", "none"]) {
            Ok(Command::Run(config)) => assert_eq!(config.center_hotkey, None),
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn control_commands_short_circuit() {
        assert_eq!(parse(&["--quit"]), Ok(Command::Quit));
        assert_eq!(parse(&["-h"]), Ok(Command::Help));
        assert_eq!(parse(&["--version"]), Ok(Command::Version));
    }

    #[test]
    fn reports_bad_input() {
        assert!(parse(&["--modifier"])
            .unwrap_err()
            .contains("requires a value"));
        assert!(parse(&["--modifier", "hyper"])
            .unwrap_err()
            .contains("unknown modifier"));
        assert!(parse(&["--bogus"]).unwrap_err().contains("unknown option"));
        assert!(parse(&["-c", "c"]).unwrap_err().contains("modifier"));
    }
}
