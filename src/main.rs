//! XDG paths, the command line, the locale, logging to the state
//! directory, opening storage, building the desktop, running the
//! terminal, and the one operation a noninteractive command is.
//!
//! The command line itself is read by `cli`, which touches nothing; what
//! is left here is the reading of a file, the opening of the database, the
//! clock and the clipboard, so that every shape of a command line is a
//! test rather than a run.

use std::fs;
use std::io::{IsTerminal, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{ExitCode, Stdio};

use jiff::Zoned;
use tracing_subscriber::EnvFilter;

use jobsdone::app::{self, App, DateOrder, Locale, WindowSize};
use jobsdone::cli::{self, Failure, Format, Operation, Parsed, Plan, Source, Then};
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
    let arguments: Vec<String> = std::env::args().skip(1).collect();

    // The format is read on its own first, so that a command line which
    // cannot be parsed is still answered in the shape it asked for.
    let asked = cli::parse::format_hint(&arguments);
    let parsed = match cli::parse(&arguments) {
        Ok(parsed) => parsed,
        Err(failure) => return complain(&failure, asked),
    };

    match parsed.plan {
        Plan::Run { notes } => match start(parsed.data_dir.as_deref(), notes) {
            Ok(()) => ExitCode::SUCCESS,
            Err(message) => {
                fail(&message);
                ExitCode::FAILURE
            }
        },
        Plan::Desktop { floating, size } => {
            let size = size.map(|(width, height)| WindowSize::new(width, height));
            match window_rule(parsed.data_dir.as_deref(), floating, size) {
                Ok(said) => {
                    print!("{}", cli::said("window_rule", &said, parsed.format));
                    ExitCode::SUCCESS
                }
                Err(message) => {
                    complain(&Failure::runtime("desktop_error", message), parsed.format)
                }
            }
        }
        Plan::Help(ref text) => {
            print!("{text}");
            ExitCode::SUCCESS
        }
        Plan::Version => {
            println!("jobsdone {}", env!("CARGO_PKG_VERSION"));
            ExitCode::SUCCESS
        }
        // The skill is compiled in, so it is answered without a database,
        // a directory that exists or anything written anywhere.
        Plan::Skill => {
            print!("{}", cli::SKILL);
            ExitCode::SUCCESS
        }
        Plan::Operate(ref operation) => match operate(operation.clone(), &parsed) {
            Ok(said) => {
                print!("{said}");
                ExitCode::SUCCESS
            }
            Err(failure) => complain(&failure, parsed.format),
        },
    }
}

/// The failure, on standard error, in the shape the command line asked
/// for. Answers go to standard output and these do not, so a script may
/// read one and log the other.
fn complain(failure: &Failure, format: Format) -> ExitCode {
    eprint!("{}", cli::report(failure, format));
    ExitCode::from(failure.exit_code)
}

/// One operation: whatever text it still wants, the database, the service,
/// and the answer written out.
fn operate(mut operation: Operation, parsed: &Parsed) -> Result<String, Failure> {
    let body = match operation.body.as_ref().map(|body| body.source.clone()) {
        Some(source) => Some(read(&source)?),
        None => None,
    };
    let input = match operation.input.clone() {
        Some(source) => Some(read(&source)?),
        None => None,
    };
    cli::complete(&mut operation, body, input)?;

    let (_, mut store) = open_the_store(parsed.data_dir.as_deref())
        .map_err(|message| Failure::runtime("storage_error", message))?;

    let locale = locale();
    let envelope = jobsdone::service::execute(
        &mut store,
        operation.request.clone(),
        &Zoned::now(),
        locale.dates,
    )
    .map_err(|error| Failure {
        code: error.code,
        message: error.message,
        exit_code: error.exit_code,
    })?;

    let dates = cli::written_order(&envelope).unwrap_or(locale.dates);
    let said = cli::present(&operation, &envelope, parsed.format, dates)?;

    if operation.then == Then::CopyNote {
        let body = cli::note_to_copy(&envelope).ok_or_else(|| {
            Failure::runtime("unreadable_response", "the answer carried no note to copy")
        })?;
        copy_to_clipboard(body).map_err(|message| Failure::runtime("clipboard_error", message))?;
    }
    Ok(said)
}

/// Text a request wants, read exactly: every newline and every byte of
/// Unicode reaches the note as it was written.
fn read(source: &Source) -> Result<String, Failure> {
    match source {
        Source::Stdin => {
            let mut text = String::new();
            std::io::stdin()
                .read_to_string(&mut text)
                .map_err(|error| {
                    Failure::runtime(
                        "input_error",
                        format!("standard input could not be read: {error}"),
                    )
                })?;
            Ok(text)
        }
        Source::File(path) => fs::read_to_string(path).map_err(|error| {
            Failure::runtime(
                "input_error",
                format!("{} could not be read: {error}", path.display()),
            )
        }),
    }
}

/// The desktop clipboard, including when the program runs inside tmux.
///
/// The window has its own copy of this in `terminal`, which owns the loop
/// that calls it; a command line is not in that loop and may not reach
/// into it, so the two spawns stand apart.
fn copy_to_clipboard(text: &str) -> Result<(), String> {
    let (program, arguments): (&str, &[&str]) = if std::env::var_os("WAYLAND_DISPLAY").is_some() {
        ("wl-copy", &["--type", "text/plain;charset=utf-8"])
    } else if std::env::var_os("DISPLAY").is_some() {
        ("xclip", &["-selection", "clipboard", "-in"])
    } else {
        return Err("there is no desktop clipboard here".to_owned());
    };

    let mut child = std::process::Command::new(program)
        .args(arguments)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|error| format!("{program} could not be run: {error}"))?;
    child
        .stdin
        .as_mut()
        .ok_or_else(|| format!("{program} took no input"))?
        .write_all(text.as_bytes())
        .map_err(|error| format!("{program} could not be written to: {error}"))?;
    match child.wait() {
        Ok(status) if status.success() => Ok(()),
        Ok(status) => Err(format!("{program} answered {status}")),
        Err(error) => Err(format!("{program} could not be waited for: {error}")),
    }
}

/// `jobsdone desktop`: the window settings the flags name, and the rule
/// written from them.
fn window_rule(
    data_dir: Option<&Path>,
    floating: Option<bool>,
    size: Option<WindowSize>,
) -> Result<String, String> {
    let (_, mut store) = open_the_store(data_dir)?;
    app::set_window(&mut store, &Hyprland::here(), floating, size)
}

fn start(data_dir: Option<&Path>, notes: bool) -> Result<(), String> {
    let state = xdg::BaseDirectories::with_prefix("jobsdone")
        .create_state_directory("")
        .map_err(|error| format!("the state directory could not be made: {error}"))?;
    start_logging(&state.join("jobsdone.log"));

    let (database, store) = open_the_store(data_dir)?;

    let mut app = App::new(
        Box::new(store),
        Box::new(Hyprland::here()),
        locale(),
        &Zoned::now(),
    )
    .map_err(|error| format!("{} could not be read: {error}", database.display()))?;
    if notes {
        app.open_on_the_notes();
    }

    terminal::run(app).map_err(|error| format!("the terminal could not be driven: {error}"))
}

/// The database, and where it was opened.
///
/// `--data-dir` stands in front of the environment override, which stands
/// in front of the XDG directory: the more deliberate the choice, the
/// further forward it is.
fn open_the_store(chosen: Option<&Path>) -> Result<(PathBuf, Sqlite), String> {
    let overridden = chosen
        .map(Path::to_path_buf)
        .or_else(|| std::env::var_os(DATA_DIR).map(PathBuf::from));
    let data = match overridden {
        Some(path) => {
            fs::create_dir_all(&path)
                .map_err(|error| format!("{} could not be made: {error}", path.display()))?;
            path
        }
        None => xdg::BaseDirectories::with_prefix("jobsdone")
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

// What the command line means is `cli`'s, and its tests are there; what is
// left here is reading a file, opening a database and spawning the
// clipboard, none of which a unit test can hold still. `tests/cli.rs`
// drives the built program instead.
