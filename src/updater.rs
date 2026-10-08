//! The release updater, as the window speaks to it: `jobsdone-update`
//! beside the binary, asked what the channel holds and told to install
//! it, each in the background.
//!
//! What the channel holds and how it stands against the installed
//! program is the updater's to say (`--check --porcelain`), so the window
//! and `jobsdone update` cannot disagree about it. What is kept here is
//! the throttle: every window on the machine shares one file in the state
//! directory that remembers when the network was last asked and what it
//! answered, so many windows and many launches a day ask it at most once
//! every fifteen minutes.

use std::fs::{self, File};
use std::io::{Read, Seek, SeekFrom, Write};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus, Stdio};
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::app::{Heard, Release, Standing, Updater};

#[cfg(test)]
mod tests;

/// What a release install puts beside the binary.
const UPDATER: &str = "jobsdone-update";

/// The binary the updater installs, beside it.
const BINARY: &str = "jobsdone";

/// The shared memory of the last request, in the state directory.
const RECORD: &str = "update-check";

/// Where an install writes what it says. Nobody watches it happen, so
/// its last line is what the window shows when it fails.
const LOG: &str = "update.log";

/// How long the network is left alone after any window has asked it.
const THROTTLE: u64 = 15 * 60;

/// How long a look may take before it is given up on. The updater's own
/// request gives up sooner; this is for a check that hangs anyway.
const PATIENCE: Duration = Duration::from_secs(30);

/// The updater beside this program, and the state directory it keeps its
/// throttle in.
pub struct Beside {
    updater: PathBuf,
    state: PathBuf,
    /// The version running here, which is the only version a remembered
    /// answer can be applied to.
    version: String,
    /// Unix seconds now. The program reads the clock; a test holds it.
    now: Arc<dyn Fn() -> u64 + Send + Sync>,
    patience: Duration,
    told: Sender<Heard>,
    hears: Receiver<Heard>,
}

impl Beside {
    /// The updater beside this program, if there is one to run.
    ///
    /// Only beside: a copy run from anywhere else, `cargo run` or a
    /// package's `/usr/bin`, is not the program the updater replaces, so
    /// it is told about no release even when `~/.local/bin` has one.
    pub fn here(state: &Path) -> Option<Beside> {
        let updater = std::env::current_exe().ok()?.parent()?.join(UPDATER);
        let runnable = fs::metadata(&updater)
            .is_ok_and(|file| file.is_file() && file.permissions().mode() & 0o111 != 0);
        runnable.then(|| {
            Beside::with(
                updater,
                state.to_path_buf(),
                env!("CARGO_PKG_VERSION"),
                Arc::new(unix_now),
                PATIENCE,
            )
        })
    }

    fn with(
        updater: PathBuf,
        state: PathBuf,
        version: &str,
        now: Arc<dyn Fn() -> u64 + Send + Sync>,
        patience: Duration,
    ) -> Beside {
        let (told, hears) = mpsc::channel();
        Beside {
            updater,
            state,
            version: version.to_owned(),
            now,
            patience,
            told,
            hears,
        }
    }

    /// The program the updater installs, which is what a finished install
    /// starts.
    pub fn binary(&self) -> PathBuf {
        self.updater.with_file_name(BINARY)
    }
}

impl Updater for Beside {
    fn look(&mut self, asked: bool) {
        let updater = self.updater.clone();
        let record = self.state.join(RECORD);
        let version = self.version.clone();
        let now = Arc::clone(&self.now);
        let patience = self.patience;
        let told = self.told.clone();
        thread::spawn(move || {
            let heard = look(&updater, &record, &version, &*now, patience, asked);
            let _ = told.send(heard);
        });
    }

    fn install(&mut self) {
        let told = self.told.clone();
        let log = self.state.join(LOG);
        let child = File::create(&log).and_then(|out| {
            let err = out.try_clone()?;
            Command::new(&self.updater)
                .stdin(Stdio::null())
                .stdout(out)
                .stderr(err)
                // Its own group, so that a window closed while it runs
                // does not take an install half done down with it.
                .process_group(0)
                .spawn()
        });
        let mut child = match child {
            Ok(child) => child,
            Err(error) => {
                let why = format!("{} could not be run: {error}", self.updater.display());
                let _ = told.send(Heard::Installed(Err(why)));
                return;
            }
        };
        thread::spawn(move || {
            let heard = match child.wait() {
                Ok(status) if status.success() => Ok(()),
                Ok(status) => {
                    let said = fs::read_to_string(&log).unwrap_or_default();
                    Err(last_line(&said).unwrap_or_else(|| stopped(status)))
                }
                Err(error) => Err(format!("the updater could not be waited for: {error}")),
            };
            let _ = told.send(Heard::Installed(heard));
        });
    }

    fn heard(&mut self) -> Option<Heard> {
        self.hears.try_recv().ok()
    }
}

fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| since.as_secs())
}

/// One look, with the record locked from the reading of it to the writing
/// of the answer, so that a second window looking at the same moment
/// waits and then finds the answer the first one heard rather than asking
/// again.
fn look(
    updater: &Path,
    path: &Path,
    version: &str,
    now: &(dyn Fn() -> u64 + Send + Sync),
    patience: Duration,
    asked: bool,
) -> Heard {
    let opened = File::options()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(path);
    let mut file = match opened {
        Ok(file) => file,
        Err(error) => {
            return Heard::Looked(Err(format!(
                "{} could not be opened: {error}",
                path.display()
            )));
        }
    };
    if let Err(error) = file.lock() {
        return Heard::Looked(Err(format!(
            "{} could not be locked: {error}",
            path.display()
        )));
    }
    let mut text = String::new();
    let _ = file.read_to_string(&mut text);
    // The clock is read once the lock is held: a window that waited on
    // another's request is judging that request's answer, not its own
    // place in the queue.
    let now = now();
    let remembered = record(&text);

    if !asked && !due(remembered.as_ref(), now) {
        return match remembered.and_then(|record| record.answer) {
            Some(release) if about(&release, version) => Heard::Looked(Ok(release)),
            _ => Heard::Nothing,
        };
    }

    let answer = ask(updater, patience);
    // A rewrite in place rather than a rename, so that the lock other
    // windows wait on is on the file they will read.
    let written = file
        .set_len(0)
        .and_then(|()| file.seek(SeekFrom::Start(0)))
        .and_then(|_| file.write_all(recorded(now, &answer).as_bytes()));
    if let Err(error) = written {
        tracing::warn!(%error, "the update check could not be remembered");
    }
    match answer {
        Ok(release) if !asked && !about(&release, version) => Heard::Nothing,
        answer => Heard::Looked(answer),
    }
}

/// `jobsdone-update --check --porcelain`, given up on after `patience`.
fn ask(updater: &Path, patience: Duration) -> Result<Release, String> {
    let mut child = Command::new(updater)
        .args(["--check", "--porcelain"])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .process_group(0)
        .spawn()
        .map_err(|error| format!("{} could not be run: {error}", updater.display()))?;

    // The answer is one short line, so nothing waits on the pipe being
    // read while this waits on the process.
    let given = std::time::Instant::now() + patience;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if std::time::Instant::now() >= given => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(format!("no answer in {} seconds", patience.as_secs()));
            }
            Ok(None) => thread::sleep(Duration::from_millis(50)),
            Err(error) => return Err(format!("the updater could not be waited for: {error}")),
        }
    };
    let mut out = String::new();
    let mut err = String::new();
    if let Some(mut pipe) = child.stdout.take() {
        let _ = pipe.read_to_string(&mut out);
    }
    if let Some(mut pipe) = child.stderr.take() {
        let _ = pipe.read_to_string(&mut err);
    }
    if !status.success() {
        return Err(last_line(&err).unwrap_or_else(|| stopped(status)));
    }
    answer(out.trim()).ok_or_else(|| format!("the updater answered \"{}\"", out.trim()))
}

/// The one line `--check --porcelain` prints: how the available release
/// stands, the installed version (`-` for none) and the available one.
fn answer(line: &str) -> Option<Release> {
    let words: Vec<&str> = line.split_whitespace().collect();
    let [standing, installed, available] = words[..] else {
        return None;
    };
    let standing = match standing {
        "newer" => Standing::Newer,
        "same" => Standing::Same,
        "older" => Standing::Older,
        _ => return None,
    };
    Some(Release {
        standing,
        installed: installed.to_owned(),
        available: available.to_owned(),
    })
}

/// The porcelain line for a release, as the record keeps it.
fn porcelain(release: &Release) -> String {
    let standing = match release.standing {
        Standing::Newer => "newer",
        Standing::Same => "same",
        Standing::Older => "older",
    };
    format!("{standing} {} {}", release.installed, release.available)
}

/// The last request any window made.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Record {
    /// When, in Unix seconds.
    at: u64,
    /// What it heard. A failure is remembered too, so that an offline
    /// machine is not asked every minute.
    answer: Option<Release>,
}

/// What the record file says. One that cannot be read is no record,
/// which is "never asked".
fn record(text: &str) -> Option<Record> {
    let line = text.lines().next()?;
    let (at, said) = line.split_once(' ')?;
    let at = at.parse().ok()?;
    let answer = match said {
        "failed" => None,
        said => Some(answer(said)?),
    };
    Some(Record { at, answer })
}

/// The record of a request made at `now`.
fn recorded(now: u64, answer: &Result<Release, String>) -> String {
    match answer {
        Ok(release) => format!("{now} {}\n", porcelain(release)),
        Err(_) => format!("{now} failed\n"),
    }
}

/// Whether the network is to be asked again: never asked, asked fifteen
/// minutes ago or more, or asked at a time still to come, which is a
/// clock that has been set back and is not to be waited out.
fn due(remembered: Option<&Record>, now: u64) -> bool {
    remembered.is_none_or(|record| record.at > now || now - record.at >= THROTTLE)
}

/// Whether an answer was given about the version running here. Another
/// window may have installed a newer one since, and its answer is about
/// that.
fn about(release: &Release, version: &str) -> bool {
    release.installed == version
}

/// The updater's own reason, which is the last thing it said, without the
/// name it signs its messages with.
fn last_line(said: &str) -> Option<String> {
    let line = said.lines().map(str::trim).rfind(|line| !line.is_empty())?;
    Some(line.strip_prefix("Jobsdone: ").unwrap_or(line).to_owned())
}

fn stopped(status: ExitStatus) -> String {
    format!("the updater stopped with {status}")
}
