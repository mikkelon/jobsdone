//! XDG paths, logging to the state directory, opening storage, running
//! the terminal.

use std::fs;
use std::io::{IsTerminal, Read, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use jiff::Zoned;
use tracing_subscriber::EnvFilter;

use jobsdone::app::{App, DateOrder, Locale};
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

fn main() -> ExitCode {
    match start() {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            fail(&message);
            ExitCode::FAILURE
        }
    }
}

fn start() -> Result<(), String> {
    let dirs = xdg::BaseDirectories::with_prefix("jobsdone");

    let state = dirs
        .create_state_directory("")
        .map_err(|error| format!("the state directory could not be made: {error}"))?;
    start_logging(&state.join("jobsdone.log"));

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

    let app = App::new(
        Box::new(store),
        Box::new(Hyprland::here()),
        locale(),
        &Zoned::now(),
    )
    .map_err(|error| format!("{} could not be read: {error}", database.display()))?;

    terminal::run(app).map_err(|error| format!("the terminal could not be driven: {error}"))
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
