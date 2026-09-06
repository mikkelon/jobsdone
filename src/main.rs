//! XDG paths, the command line, the locale, logging to the state
//! directory, opening storage, building the desktop, running the
//! terminal.

use std::fs;
use std::io::{IsTerminal, Read, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use jiff::Zoned;
use tracing_subscriber::EnvFilter;

use jobsdone::app::{self, App, DateOrder, Locale, WindowSize};
use jobsdone::desktop::Hyprland;
use jobsdone::storage::Sqlite;
use jobsdone::terminal;

/// A development override, so `make run` never opens the real database.
const DATA_DIR: &str = "JOBSDONE_DATA_DIR";
/// The log level, in the program's own namespace rather than RUST_LOG, so
/// a variable set for another tool cannot change what this one writes.
const LOG_LEVEL: &str = "JOBSDONE_LOG";
/// Past this the log is started again. It is read roughly never and the
/// program is launched many times a day.
const LOG_LIMIT: u64 = 1024 * 1024;

/// What the locale is read from, most specific first (STACK.md section
/// 8).
const LOCALE: [&str; 3] = ["LC_ALL", "LC_TIME", "LANG"];

/// The territories that write the month before the day. Everywhere else,
/// and `C`, `POSIX` and an unset locale, writes the day first, which is
/// the form the wireframes are drawn in.
const MONTH_FIRST: [&str; 11] = [
    "US", "PH", "FM", "MH", "PW", "GU", "PR", "VI", "AS", "MP", "UM",
];

/// What `--help` prints, and what a command line nobody can read is
/// answered with.
const USAGE: &str = "\
jobsdone, a keyboard-first daily task manager for the terminal

    jobsdone                                open the app
    jobsdone desktop [--floating | --tiled] [--size WxH]
                                            write the window rule
    jobsdone --help
    jobsdone --version

The desktop command puts the flags it is given in the settings, writes
the window rule the window manager reads and reloads it. A flag left off
keeps the setting as it is.
";

/// The exit code of a command line that could not be read, as distinct
/// from the program having run and failed.
const MISUSE: u8 = 2;

fn main() -> ExitCode {
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    match invocation(arguments.iter().map(String::as_str)) {
        Ok(Invocation::Run) => match start() {
            Ok(()) => ExitCode::SUCCESS,
            Err(message) => {
                fail(&message);
                ExitCode::FAILURE
            }
        },
        Ok(Invocation::Window { floating, size }) => match window_rule(floating, size) {
            Ok(said) => {
                println!("{said}");
                ExitCode::SUCCESS
            }
            Err(message) => {
                eprintln!("jobsdone: {message}");
                ExitCode::FAILURE
            }
        },
        Ok(Invocation::Help) => {
            print!("{USAGE}");
            ExitCode::SUCCESS
        }
        Ok(Invocation::Version) => {
            println!("jobsdone {}", env!("CARGO_PKG_VERSION"));
            ExitCode::SUCCESS
        }
        Err(message) => {
            eprintln!("jobsdone: {message}\n");
            eprint!("{USAGE}");
            ExitCode::from(MISUSE)
        }
    }
}

/// What a command line asks the program to do.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Invocation {
    Run,
    /// `desktop`, with the window the flags asked for: `None` where a
    /// flag was left off and the setting stands.
    Window {
        floating: Option<bool>,
        size: Option<WindowSize>,
    },
    Help,
    Version,
}

/// The command line, read. Nothing here touches the world, so every form
/// of the command is a test rather than a run.
fn invocation<'a>(arguments: impl IntoIterator<Item = &'a str>) -> Result<Invocation, String> {
    let mut arguments = arguments.into_iter();
    let Some(command) = arguments.next() else {
        return Ok(Invocation::Run);
    };
    match command {
        "--help" | "-h" => Ok(Invocation::Help),
        "--version" | "-V" => Ok(Invocation::Version),
        "desktop" => window(arguments),
        _ => Err(format!("{command} is not a command")),
    }
}

/// The flags of `jobsdone desktop`.
fn window<'a>(arguments: impl IntoIterator<Item = &'a str>) -> Result<Invocation, String> {
    let mut arguments = arguments.into_iter();
    let mut floating: Option<bool> = None;
    let mut size = None;

    while let Some(argument) = arguments.next() {
        match argument {
            "--floating" | "--tiled" => {
                let wanted = argument == "--floating";
                if floating.is_some_and(|asked| asked != wanted) {
                    return Err("--floating and --tiled ask for different windows".to_owned());
                }
                floating = Some(wanted);
            }
            "--size" => {
                let value = arguments
                    .next()
                    .ok_or_else(|| "--size wants a size, as in --size 870x650".to_owned())?;
                size = Some(
                    pixels(value)
                        .ok_or_else(|| format!("{value} is not a size, as in --size 870x650"))?,
                );
            }
            "--help" | "-h" => return Ok(Invocation::Help),
            _ => return Err(format!("{argument} is not a flag of the desktop command")),
        }
    }
    Ok(Invocation::Window { floating, size })
}

/// `870x650` in logical pixels. A number outside what a window may be is
/// held to the range, the way the settings page holds one.
fn pixels(text: &str) -> Option<WindowSize> {
    let (width, height) = text.split_once('x')?;
    Some(WindowSize::new(
        width.trim().parse().ok()?,
        height.trim().parse().ok()?,
    ))
}

/// `jobsdone desktop`: the window settings the flags name, and the rule
/// written from them.
fn window_rule(floating: Option<bool>, size: Option<WindowSize>) -> Result<String, String> {
    let dirs = xdg::BaseDirectories::with_prefix("jobsdone");
    let (_, mut store) = open_the_store(&dirs)?;
    app::set_window(&mut store, &Hyprland::here(), floating, size)
}

fn start() -> Result<(), String> {
    let dirs = xdg::BaseDirectories::with_prefix("jobsdone");

    let state = dirs
        .create_state_directory("")
        .map_err(|error| format!("the state directory could not be made: {error}"))?;
    start_logging(&state.join("jobsdone.log"));

    let (database, store) = open_the_store(&dirs)?;

    let app = App::new(
        Box::new(store),
        Box::new(Hyprland::here()),
        locale(),
        &Zoned::now(),
    )
    .map_err(|error| format!("{} could not be read: {error}", database.display()))?;

    terminal::run(app).map_err(|error| format!("the terminal could not be driven: {error}"))
}

/// The database, and where it was opened.
fn open_the_store(dirs: &xdg::BaseDirectories) -> Result<(PathBuf, Sqlite), String> {
    let data = match std::env::var_os(DATA_DIR) {
        Some(overridden) => {
            let path = PathBuf::from(overridden);
            fs::create_dir_all(&path)
                .map_err(|error| format!("{DATA_DIR} could not be made: {error}"))?;
            path
        }
        None => dirs
            .create_data_directory("")
            .map_err(|error| format!("the data directory could not be made: {error}"))?,
    };

    // The error type here belongs to the domain, which main may not name.
    // It displays itself, which is all a message needs.
    let database = data.join("jobsdone.db");
    let store = Sqlite::open(&database)
        .map_err(|error| format!("{} could not be opened: {error}", database.display()))?;
    Ok((database, store))
}

/// What the environment says dates look like here.
fn locale() -> Locale {
    let text = LOCALE
        .iter()
        .find_map(|name| std::env::var(name).ok().filter(|value| !value.is_empty()))
        .unwrap_or_default();
    Locale {
        dates: date_order(&text),
    }
}

/// The order of a locale name: `en_US.UTF-8` is the territory `US`, and
/// a name with no territory in it is nobody's month-first.
fn date_order(locale: &str) -> DateOrder {
    let territory = locale
        .split(['.', '@'])
        .next()
        .unwrap_or_default()
        .split(['_', '-'])
        .nth(1)
        .unwrap_or_default()
        .to_uppercase();
    if MONTH_FIRST.contains(&territory.as_str()) {
        DateOrder::MonthFirst
    } else {
        DateOrder::DayFirst
    }
}

/// Diagnostics go to a file. A program launched from a keybind has no
/// stderr anyone reads.
fn start_logging(path: &Path) {
    if fs::metadata(path).is_ok_and(|file| file.len() > LOG_LIMIT) {
        let _ = fs::remove_file(path);
    }

    let Ok(file) = fs::OpenOptions::new().create(true).append(true).open(path) else {
        return;
    };
    let filter = EnvFilter::try_from_env(LOG_LEVEL).unwrap_or_else(|_| EnvFilter::new("info"));

    let _ = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(std::sync::Mutex::new(file))
        .with_ansi(false)
        .try_init();
}

/// The one thing that is written to stderr on purpose.
///
/// A keybind opens a terminal that closes the moment this process exits,
/// taking the message with it, so when there is somebody there to read it
/// the window is held open until they have.
fn fail(message: &str) {
    tracing::error!(message, "jobsdone could not start");
    eprintln!("jobsdone: {message}");

    if std::io::stderr().is_terminal() && std::io::stdin().is_terminal() {
        eprint!("Press Enter to close. ");
        let _ = std::io::stderr().flush();
        let _ = std::io::stdin().read(&mut [0u8]);
    }
}

/// The command line is the only thing here worth a test, and it is read
/// by a pure function so that it can have one. The tests are inline
/// because a `src/main/tests.rs` would make `main` a second top-level
/// module, which the table in ARCHITECTURE.md section 2 does not list.
#[cfg(test)]
mod tests {
    use super::*;

    fn read(arguments: &[&str]) -> Result<Invocation, String> {
        invocation(arguments.iter().copied())
    }

    #[test]
    fn nothing_on_the_command_line_opens_the_app() {
        assert_eq!(read(&[]), Ok(Invocation::Run));
    }

    #[test]
    fn the_desktop_command_carries_the_window_the_flags_asked_for() {
        assert_eq!(
            read(&["desktop"]),
            Ok(Invocation::Window {
                floating: None,
                size: None
            })
        );
        assert_eq!(
            read(&["desktop", "--tiled"]),
            Ok(Invocation::Window {
                floating: Some(false),
                size: None
            })
        );
        assert_eq!(
            read(&["desktop", "--floating", "--size", "1000x700"]),
            Ok(Invocation::Window {
                floating: Some(true),
                size: Some(WindowSize::new(1000, 700))
            })
        );
    }

    #[test]
    fn a_size_outside_what_a_window_may_be_is_held_to_the_range() {
        assert_eq!(
            read(&["desktop", "--size", "10x99999"]),
            Ok(Invocation::Window {
                floating: None,
                size: Some(WindowSize {
                    width: 200,
                    height: 10_000
                })
            })
        );
    }

    #[test]
    fn a_floating_window_and_a_tiled_one_cannot_both_be_asked_for() {
        assert!(read(&["desktop", "--floating", "--tiled"]).is_err());
        // The same one twice is nobody contradicting themselves.
        assert!(read(&["desktop", "--tiled", "--tiled"]).is_ok());
    }

    #[test]
    fn a_size_that_is_not_one_is_refused() {
        assert!(read(&["desktop", "--size", "roomy"]).is_err());
        assert!(read(&["desktop", "--size", "1000"]).is_err());
        assert!(read(&["desktop", "--size"]).is_err());
    }

    #[test]
    fn help_and_version_are_asked_for_by_themselves_or_after_a_command() {
        assert_eq!(read(&["--help"]), Ok(Invocation::Help));
        assert_eq!(read(&["-h"]), Ok(Invocation::Help));
        assert_eq!(read(&["--version"]), Ok(Invocation::Version));
        assert_eq!(read(&["desktop", "--help"]), Ok(Invocation::Help));
    }

    #[test]
    fn anything_else_is_refused_by_name() {
        assert_eq!(read(&["fly"]), Err("fly is not a command".to_owned()));
        assert_eq!(
            read(&["desktop", "--quickly"]),
            Err("--quickly is not a flag of the desktop command".to_owned())
        );
    }
}
