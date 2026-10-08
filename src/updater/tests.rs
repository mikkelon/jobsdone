use super::*;

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

use tempfile::TempDir;

fn newer() -> Release {
    Release {
        standing: Standing::Newer,
        installed: "1.4.0".to_owned(),
        available: "1.5.0".to_owned(),
    }
}

#[test]
fn the_porcelain_line_is_a_standing_and_two_versions() {
    assert_eq!(answer("newer 1.4.0 1.5.0"), Some(newer()));
    assert_eq!(
        answer("same 1.5.0-beta.2 1.5.0-beta.2"),
        Some(Release {
            standing: Standing::Same,
            installed: "1.5.0-beta.2".to_owned(),
            available: "1.5.0-beta.2".to_owned(),
        })
    );
    assert_eq!(
        answer("older 1.6.0 1.5.0"),
        Some(Release {
            standing: Standing::Older,
            installed: "1.6.0".to_owned(),
            available: "1.5.0".to_owned(),
        })
    );
    assert_eq!(
        answer("newer - 1.5.0").map(|release| release.installed),
        Some("-".to_owned())
    );
}

#[test]
fn anything_else_on_the_line_is_not_an_answer() {
    for line in [
        "",
        "newer 1.5.0",
        "newer 1.4.0 1.5.0 extra",
        "maybe 1.4.0 1.5.0",
        "Jobsdone 1.5.0 is up to date.",
    ] {
        assert_eq!(answer(line), None, "{line:?}");
    }
}

#[test]
fn the_record_is_when_and_what_was_heard() {
    assert_eq!(
        record("1700000000 newer 1.4.0 1.5.0\n"),
        Some(Record {
            at: 1_700_000_000,
            answer: Some(newer()),
        })
    );
    assert_eq!(
        record("1700000000 failed\n"),
        Some(Record {
            at: 1_700_000_000,
            answer: None,
        })
    );
}

#[test]
fn a_record_that_cannot_be_read_is_never_asked() {
    for text in [
        "",
        "\n",
        "soon newer 1.4.0 1.5.0",
        "1700000000",
        "1700000000 maybe",
    ] {
        assert_eq!(record(text), None, "{text:?}");
    }
}

#[test]
fn a_request_is_written_down_the_way_it_is_read_back() {
    assert_eq!(
        recorded(1_700_000_000, &Ok(newer())),
        "1700000000 newer 1.4.0 1.5.0\n"
    );
    assert_eq!(
        recorded(1_700_000_000, &Err("offline".to_owned())),
        "1700000000 failed\n"
    );
    assert_eq!(
        record(&recorded(42, &Ok(newer()))),
        Some(Record {
            at: 42,
            answer: Some(newer()),
        })
    );
}

#[test]
fn the_network_is_asked_again_fifteen_minutes_after_the_last_request() {
    let asked_at = |at| Record { at, answer: None };
    assert!(due(None, 1_000));
    assert!(!due(Some(&asked_at(1_000)), 1_000));
    assert!(!due(Some(&asked_at(1_000)), 1_899));
    assert!(due(Some(&asked_at(1_000)), 1_900));
    // A clock set back is not waited out.
    assert!(due(Some(&asked_at(5_000)), 1_000));
}

#[test]
fn an_answer_applies_only_to_the_version_it_was_about() {
    assert!(about(&newer(), "1.4.0"));
    assert!(!about(&newer(), "1.3.0"));
}

#[test]
fn the_updaters_reason_is_its_last_line_without_its_name() {
    assert_eq!(
        last_line("Downloading\nJobsdone: Download failed. Your installation has not changed.\n\n"),
        Some("Download failed. Your installation has not changed.".to_owned())
    );
    assert_eq!(
        last_line("  plain words  \n"),
        Some("plain words".to_owned())
    );
    assert_eq!(last_line("\n \n"), None);
}

// ---- the updater as a process -----------------------------------------

/// A state directory and a fake `jobsdone-update` that counts its runs.
struct Scene {
    dir: TempDir,
    clock: Arc<AtomicU64>,
}

impl Scene {
    /// `body` is the shell the fake runs after it has counted itself.
    fn new(body: &str) -> Scene {
        let dir = tempfile::tempdir().unwrap();
        let updater = dir.path().join(UPDATER);
        let script = format!(
            "#!/bin/sh\n[ \"$1\" = probe ] && exit 0\necho \"$*\" >> \"$(dirname \"$0\")/runs\"\n{body}\n"
        );
        fs::write(&updater, script).unwrap();
        fs::set_permissions(&updater, fs::Permissions::from_mode(0o755)).unwrap();
        // Another test forking while the script was open for writing can
        // hold it open for a moment, and the kernel refuses to run a file
        // that is; one run that starts is the moment that has passed.
        for _ in 0..50 {
            match Command::new(&updater).arg("probe").status() {
                Err(error) if error.kind() == std::io::ErrorKind::ExecutableFileBusy => {
                    thread::sleep(Duration::from_millis(20));
                }
                result => {
                    result.unwrap();
                    break;
                }
            }
        }
        Scene {
            dir,
            clock: Arc::new(AtomicU64::new(1_700_000_000)),
        }
    }

    fn beside(&self, version: &str) -> Beside {
        self.patient(version, PATIENCE)
    }

    fn patient(&self, version: &str, patience: Duration) -> Beside {
        let clock = Arc::clone(&self.clock);
        Beside::with(
            self.dir.path().join(UPDATER),
            self.dir.path().to_path_buf(),
            version,
            Arc::new(move || clock.load(Ordering::SeqCst)),
            patience,
        )
    }

    fn later(&self, seconds: u64) {
        self.clock.fetch_add(seconds, Ordering::SeqCst);
    }

    /// The arguments of every run, one line each.
    fn runs(&self) -> Vec<String> {
        fs::read_to_string(self.dir.path().join("runs"))
            .unwrap_or_default()
            .lines()
            .map(str::to_owned)
            .collect()
    }

    fn file(&self, name: &str) -> String {
        fs::read_to_string(self.dir.path().join(name)).unwrap_or_default()
    }
}

fn wait(beside: &mut Beside) -> Heard {
    let until = Instant::now() + Duration::from_secs(10);
    while Instant::now() < until {
        if let Some(heard) = beside.heard() {
            return heard;
        }
        thread::sleep(Duration::from_millis(10));
    }
    panic!("the updater said nothing in ten seconds");
}

#[test]
fn windows_share_one_request_every_fifteen_minutes() {
    let scene = Scene::new("echo 'newer 1.4.0 1.5.0'");
    let mut first = scene.beside("1.4.0");
    let mut second = scene.beside("1.4.0");

    first.look(false);
    assert_eq!(wait(&mut first), Heard::Looked(Ok(newer())));
    assert_eq!(scene.runs(), ["--check --porcelain"]);
    assert_eq!(scene.file(RECORD), "1700000000 newer 1.4.0 1.5.0\n");

    // Another window a minute later hears what the first one did.
    scene.later(60);
    second.look(false);
    assert_eq!(wait(&mut second), Heard::Looked(Ok(newer())));
    assert_eq!(scene.runs().len(), 1);

    // Fifteen minutes after the request, the next look asks again.
    scene.later(15 * 60 - 60);
    first.look(false);
    assert_eq!(wait(&mut first), Heard::Looked(Ok(newer())));
    assert_eq!(scene.runs().len(), 2);
    assert_eq!(scene.file(RECORD), "1700000900 newer 1.4.0 1.5.0\n");
}

#[test]
fn two_windows_looking_at_once_ask_once() {
    let scene = Scene::new("sleep 0.3\necho 'newer 1.4.0 1.5.0'");
    let mut first = scene.beside("1.4.0");
    let mut second = scene.beside("1.4.0");

    first.look(false);
    second.look(false);
    assert_eq!(wait(&mut first), Heard::Looked(Ok(newer())));
    assert_eq!(wait(&mut second), Heard::Looked(Ok(newer())));
    assert_eq!(scene.runs().len(), 1);
}

#[test]
fn a_look_that_was_asked_for_always_asks() {
    let scene = Scene::new("echo 'newer 1.4.0 1.5.0'");
    let mut beside = scene.beside("1.4.0");

    beside.look(false);
    wait(&mut beside);
    beside.look(true);
    assert_eq!(wait(&mut beside), Heard::Looked(Ok(newer())));
    assert_eq!(scene.runs().len(), 2);
}

#[test]
fn an_answer_about_another_version_is_nothing_to_this_one() {
    let scene = Scene::new("echo 'newer 1.4.0 1.5.0'");
    let mut old = scene.beside("1.3.0");
    old.look(false);
    assert_eq!(wait(&mut old), Heard::Nothing);

    old.look(false);
    assert_eq!(wait(&mut old), Heard::Nothing);
    assert_eq!(scene.runs().len(), 1);

    // Asked, it says what the updater said.
    old.look(true);
    assert_eq!(wait(&mut old), Heard::Looked(Ok(newer())));
}

#[test]
fn a_failed_request_is_its_reason_and_is_not_repeated_for_a_while() {
    let scene = Scene::new(
        "echo 'Jobsdone: Could not find a stable release. Check your connection and try again.' >&2\nexit 1",
    );
    let mut beside = scene.beside("1.4.0");

    beside.look(true);
    assert_eq!(
        wait(&mut beside),
        Heard::Looked(Err(
            "Could not find a stable release. Check your connection and try again.".to_owned()
        ))
    );
    assert_eq!(scene.file(RECORD), "1700000000 failed\n");

    scene.later(60);
    beside.look(false);
    assert_eq!(wait(&mut beside), Heard::Nothing);
    assert_eq!(scene.runs().len(), 1);
}

#[test]
fn a_look_that_hangs_is_given_up_on() {
    let scene = Scene::new("sleep 20");
    let mut beside = scene.patient("1.4.0", Duration::from_secs(2));
    let started = Instant::now();
    beside.look(true);
    assert_eq!(
        wait(&mut beside),
        Heard::Looked(Err("no answer in 2 seconds".to_owned()))
    );
    assert!(started.elapsed() < Duration::from_secs(6));
}

#[test]
fn an_install_runs_the_updater_alone_and_hears_how_it_went() {
    let scene = Scene::new("echo 'Installed jobsdone 1.5.0'");
    let mut beside = scene.beside("1.4.0");
    beside.install();
    assert_eq!(wait(&mut beside), Heard::Installed(Ok(())));
    assert_eq!(scene.runs(), [""]);
    assert_eq!(scene.file(LOG), "Installed jobsdone 1.5.0\n");
}

#[test]
fn a_failed_install_is_the_updaters_last_line() {
    let scene = Scene::new(
        "echo 'Downloading'\necho 'Jobsdone: Download failed. Your installation has not changed.' >&2\nexit 1",
    );
    let mut beside = scene.beside("1.4.0");
    beside.install();
    assert_eq!(
        wait(&mut beside),
        Heard::Installed(Err(
            "Download failed. Your installation has not changed.".to_owned()
        ))
    );
}

#[test]
fn an_install_that_says_nothing_says_how_it_stopped() {
    let scene = Scene::new("exit 3");
    let mut beside = scene.beside("1.4.0");
    beside.install();
    assert_eq!(
        wait(&mut beside),
        Heard::Installed(Err("the updater stopped with exit status: 3".to_owned()))
    );
}

#[test]
fn the_binary_to_start_is_the_one_beside_the_updater() {
    let scene = Scene::new("");
    assert_eq!(
        scene.beside("1.4.0").binary(),
        scene.dir.path().join("jobsdone")
    );
}
