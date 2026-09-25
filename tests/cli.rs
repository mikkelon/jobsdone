//! The built program, driven the way anything else drives it: a process, a
//! command line, a scratch database, and what came back on each of the two
//! streams. What a command line *means* is tested in `src/cli/tests.rs`,
//! which needs no process; this is about the parts that only exist once
//! there is one — the exit code, which stream a thing went to, a file, a
//! pipe, and the database on the other side of it.
//!
//! Every test makes its own database in a temporary directory. Nothing
//! here reads or writes the real one, and nothing here opens the window or
//! touches the window manager.

use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};

use serde_json::Value;
use tempfile::TempDir;

/// What one run of the program came to.
struct Run {
    code: i32,
    out: String,
    err: String,
}

impl Run {
    /// The answer as the machine response it is, which also holds the run
    /// to having said it on standard output and nothing else.
    fn json(&self) -> Value {
        assert!(self.err.is_empty(), "stderr: {}", self.err);
        assert_eq!(self.out.lines().count(), 1, "one line: {:?}", self.out);
        serde_json::from_str(&self.out)
            .unwrap_or_else(|error| panic!("{:?} is not JSON: {error}", self.out))
    }

    /// The failure as the machine response it is, on standard error.
    fn failure(&self) -> Value {
        assert!(self.out.is_empty(), "stdout: {}", self.out);
        serde_json::from_str(&self.err)
            .unwrap_or_else(|error| panic!("{:?} is not JSON: {error}", self.err))
    }
}

fn run(data: &Path, arguments: &[&str], stdin: Option<&str>) -> Run {
    let mut child = Command::new(env!("CARGO_BIN_EXE_jobsdone"))
        .arg("--data-dir")
        .arg(data)
        .args(arguments)
        // The environment override must not reach in from whoever ran the
        // tests, and neither must a locale: a date in the output would
        // otherwise be written the way the machine happens to write one.
        .env_remove("JOBSDONE_DATA_DIR")
        .env("LC_ALL", "C")
        .env("LANG", "C")
        .env_remove("LC_TIME")
        .stdin(if stdin.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the program should be runnable");

    if let Some(text) = stdin {
        child
            .stdin
            .as_mut()
            .expect("piped")
            .write_all(text.as_bytes())
            .expect("stdin should take the text");
    }
    let done = child.wait_with_output().expect("the program should finish");
    Run {
        code: done.status.code().unwrap_or(-1),
        out: String::from_utf8(done.stdout).expect("stdout is UTF-8"),
        err: String::from_utf8(done.stderr).expect("stderr is UTF-8"),
    }
}

/// A run that was meant to work.
fn ok(data: &Path, arguments: &[&str]) -> Run {
    let done = run(data, arguments, None);
    assert_eq!(done.code, 0, "{arguments:?} said {}", done.err);
    done
}

fn scratch() -> TempDir {
    tempfile::tempdir().expect("a temporary directory")
}

/// A directory that cannot be made, for the commands that must answer
/// without one.
fn impossible(within: &TempDir) -> std::path::PathBuf {
    let file = within.path().join("a-file");
    std::fs::write(&file, b"not a directory").expect("a file");
    file.join("inside")
}

// ---- what needs no database -----------------------------------------

#[test]
fn help_and_version_and_the_skill_are_answered_without_a_database() {
    let dir = scratch();
    let nowhere = impossible(&dir);

    let help = run(&nowhere, &["--help"], None);
    assert_eq!(help.code, 0, "{}", help.err);
    assert!(help.out.contains("task add"), "{}", help.out);
    assert!(help.out.contains("undo apply"), "{}", help.out);
    assert!(
        help.out.contains("--skill"),
        "the help should say --skill is there"
    );
    assert!(help.err.is_empty());

    let version = run(&nowhere, &["--version"], None);
    assert_eq!(version.code, 0);
    assert!(version.out.starts_with("jobsdone "), "{}", version.out);

    let skill = run(&nowhere, &["--skill"], None);
    assert_eq!(skill.code, 0, "{}", skill.err);
    assert!(
        skill.out.starts_with("---\n"),
        "front matter: {:?}",
        &skill.out[..40.min(skill.out.len())]
    );
    assert!(skill.out.contains("name: jobsdone"));
    assert!(skill.out.contains("# Jobsdone"));
    assert!(skill.err.is_empty());

    // Nothing was made where nothing could be made.
    assert!(!nowhere.exists());
}

#[test]
fn a_command_s_own_help_says_what_it_takes_and_what_it_sends() {
    let dir = scratch();
    let nowhere = impossible(&dir);
    let help = run(&nowhere, &["task", "add", "--help"], None);
    assert_eq!(help.code, 0);
    assert!(help.out.contains("jobsdone task add TITLE"), "{}", help.out);
    assert!(help.out.contains("task.add"), "{}", help.out);
}

// ---- a command line nobody can read ---------------------------------

#[test]
fn a_command_line_that_cannot_be_read_is_two_and_says_so_on_standard_error() {
    let dir = scratch();
    for arguments in [
        vec!["fly"],
        vec!["task", "frobnicate"],
        vec!["task", "add", "x", "--quickly"],
        vec!["task", "close", "3", "3"],
        vec!["settings", "set", "colour=blue"],
    ] {
        let done = run(dir.path(), &arguments, None);
        assert_eq!(done.code, 2, "{arguments:?} said {}", done.out);
        assert!(done.out.is_empty(), "{arguments:?} wrote {}", done.out);
        assert!(done.err.starts_with("jobsdone: "), "{}", done.err);
    }
}

#[test]
fn toon_is_not_a_format_this_program_writes() {
    let dir = scratch();
    let done = run(dir.path(), &["task", "list", "--format", "toon"], None);
    assert_eq!(done.code, 2);
    assert!(done.err.contains("toon"), "{}", done.err);
    assert!(done.err.contains("text and json"), "{}", done.err);
}

#[test]
fn a_failure_is_written_in_the_format_that_was_asked_for_either_way_round() {
    let dir = scratch();
    for arguments in [vec!["--json", "task", "fly"], vec!["task", "fly", "--json"]] {
        let done = run(dir.path(), &arguments, None);
        assert_eq!(done.code, 2);
        let failure = done.failure();
        assert_eq!(failure["ok"], false);
        assert_eq!(failure["schema_version"], 1);
        assert!(failure["error"]["message"].is_string(), "{failure}");
    }
}

// ---- the exit codes -------------------------------------------------

#[test]
fn the_exit_code_says_which_kind_of_no_it_was() {
    let dir = scratch();
    let data = dir.path();
    ok(data, &["task", "add", "A task", "--today"]);

    // A request the service cannot read at all.
    let unknown = run(
        data,
        &["--json", "task", "add", "x", "--today", "--input", "-"],
        Some(r#"{"colour":"blue"}"#),
    );
    assert_eq!(unknown.code, 2, "{}", unknown.out);
    assert_eq!(unknown.failure()["error"]["code"], "invalid_request");

    // Something that is not there.
    let missing = run(data, &["--json", "task", "get", "999"], None);
    assert_eq!(missing.code, 3, "{}", missing.out);
    assert_eq!(missing.failure()["error"]["code"], "not_found");

    // A change the rules refused.
    let refused = run(data, &["--json", "task", "add", "   ", "--today"], None);
    assert_eq!(refused.code, 4, "{} {}", refused.out, refused.err);
    assert_eq!(refused.failure()["error"]["code"], "rejected");

    // Somebody else got there first, which is what the guard on undo is
    // for: the entry named is not the one on top any more.
    let stale = run(data, &["--json", "undo", "apply", "--entry", "99"], None);
    assert_eq!(stale.code, 5, "{} {}", stale.out, stale.err);
    assert_eq!(stale.failure()["error"]["code"], "conflict");
    assert!(
        !ok(data, &["--json", "undo", "get"]).json()["data"]["entry"].is_null(),
        "a refused undo takes nothing off the stack"
    );
}

// ---- the two formats ------------------------------------------------

#[test]
fn the_machine_response_is_one_line_of_the_service_s_own_envelope() {
    let dir = scratch();
    let added = ok(dir.path(), &["--json", "task", "add", "A task", "--today"]);
    let envelope = added.json();
    assert_eq!(envelope["schema_version"], 1);
    assert_eq!(envelope["ok"], true);
    assert!(envelope["context"]["today"].is_string());
    assert!(envelope["context"]["recurrence_pending"].is_boolean());
    assert!(envelope["data"]["task"]["id"].is_number());
}

#[test]
fn a_reading_of_an_answer_is_a_list_rather_than_the_envelope() {
    let dir = scratch();
    ok(
        dir.path(),
        &["task", "add", "Write the report", "--today", "--focus"],
    );
    let day = ok(dir.path(), &["day", "get"]);
    assert!(day.out.contains("Write the report"), "{}", day.out);
    assert!(day.out.contains("Focus"), "{}", day.out);
    // Not the envelope, and nothing coloured.
    assert!(!day.out.contains("schema_version"), "{}", day.out);
    assert!(!day.out.contains('{'), "{}", day.out);
    assert!(!day.out.contains('\u{1b}'), "no escape sequences");
}

// ---- one invocation is one change -----------------------------------

#[test]
fn everything_a_new_task_starts_with_is_one_change_and_one_undo() {
    let dir = scratch();
    let data = dir.path();
    let added = ok(
        data,
        &[
            "--json",
            "task",
            "add",
            "Write the report",
            "--today",
            "--focus",
            "--due",
            "2026-12-24",
            "--remind",
            "2026-12-20",
        ],
    )
    .json();
    let task = &added["data"]["task"];
    assert_eq!(task["focus"], true);
    assert_eq!(task["due_on"], "2026-12-24");
    assert_eq!(task["remind_on"], "2026-12-20");
    assert_eq!(task["place"]["kind"], "day");

    // One entry behind all of it, and one undo that takes all of it back.
    let entry = added["data"]["undo"]["id"].as_i64().expect("an undo entry");
    assert_eq!(
        ok(data, &["--json", "undo", "get"]).json()["data"]["depth"],
        1
    );

    let undone = ok(
        data,
        &["--json", "undo", "apply", "--entry", &entry.to_string()],
    )
    .json();
    assert_eq!(undone["data"]["applied"], true);
    assert_eq!(undone["data"]["depth"], 0);
    assert_eq!(
        ok(data, &["--json", "task", "list"]).json()["data"]["total"],
        0
    );
}

#[test]
fn several_tasks_close_in_one_change_and_come_back_in_one_undo() {
    let dir = scratch();
    let data = dir.path();
    for title in ["One", "Two", "Three"] {
        ok(data, &["task", "add", title, "--today"]);
    }
    let closed = ok(data, &["--json", "task", "close", "1", "2", "3"]).json();
    assert_eq!(closed["data"]["count"], 3);
    assert!(!closed["data"]["undo"]["id"].is_null());

    ok(data, &["undo", "apply"]);
    let day = ok(data, &["--json", "day", "get"]).json();
    assert_eq!(day["data"]["counts"]["open"], 3);
    assert_eq!(day["data"]["counts"]["done"], 0);
}

#[test]
fn a_multi_id_change_that_is_refused_anywhere_writes_nothing() {
    let dir = scratch();
    let data = dir.path();
    ok(data, &["task", "add", "One", "--today"]);
    ok(data, &["task", "add", "Two", "--today"]);

    let refused = run(
        data,
        &["--json", "task", "move", "1", "2", "999", "--to", "backlog"],
        None,
    );
    assert!(refused.code >= 2, "{}", refused.out);
    // Neither of the two that existed moved.
    let day = ok(data, &["--json", "day", "get"]).json();
    assert_eq!(day["data"]["counts"]["open"], 2, "{day}");
    assert_eq!(
        ok(data, &["--json", "undo", "get"]).json()["data"]["depth"],
        2,
        "no entry for a change that did not happen"
    );
}

#[test]
fn a_whole_day_is_reordered_with_one_order_and_nothing_less_will_do() {
    let dir = scratch();
    let data = dir.path();
    for title in ["One", "Two", "Three"] {
        ok(data, &["task", "add", title, "--today"]);
    }

    let reordered = ok(data, &["--json", "day", "reorder", "--order", "3,1,2"]).json();
    let order: Vec<i64> = reordered["data"]["order"]
        .as_array()
        .expect("an order")
        .iter()
        .map(|task| task["id"].as_i64().expect("an id"))
        .collect();
    assert_eq!(order, vec![3, 1, 2]);

    // A partial order, a duplicate and a stranger are each refused. Which
    // kind of no it is differs — a duplicate is a request that cannot be
    // read and a stranger is a task that is not there — but none of them
    // is a yes.
    for order in ["3,1", "3,1,1", "3,1,2,9"] {
        let refused = run(data, &["--json", "day", "reorder", "--order", order], None);
        assert!(refused.code >= 2, "{order} was taken: {}", refused.out);
        assert!(refused.out.is_empty(), "{order} wrote {}", refused.out);
    }
    let still = ok(data, &["--json", "day", "get"]).json();
    let ids: Vec<i64> = still["data"]["plan"]
        .as_array()
        .expect("a plan")
        .iter()
        .map(|row| row["id"].as_i64().expect("an id"))
        .collect();
    assert_eq!(ids, vec![3, 1, 2], "nothing moved after the refusals");
}

#[test]
fn one_task_is_reordered_against_another_without_naming_the_rest() {
    let dir = scratch();
    let data = dir.path();
    for title in ["One", "Two", "Three"] {
        ok(data, &["task", "add", title, "--today"]);
    }
    let moved = ok(data, &["--json", "task", "reorder", "3", "--before", "1"]).json();
    let order: Vec<i64> = moved["data"]["order"]
        .as_array()
        .expect("an order")
        .iter()
        .map(|task| task["id"].as_i64().expect("an id"))
        .collect();
    assert_eq!(order, vec![3, 1, 2]);
    assert_eq!(
        ok(data, &["--json", "task", "reorder", "3", "--top"]).json()["ok"],
        true
    );
}

#[test]
fn a_note_is_taken_out_of_spell_checking_from_the_command_line() {
    let dir = scratch();
    let data = dir.path();
    ok(data, &["note", "create", "Xqzt vrrbl"]);

    let first = |out: &str| out.lines().next().unwrap_or_default().to_owned();
    let read = ok(data, &["note", "get", "1"]);
    assert!(!first(&read.out).contains("no spell check"), "{}", read.out);

    let switched = ok(data, &["note", "spell-check", "1", "off"]);
    assert!(
        switched.out.contains("Spell check off for note 1."),
        "{}",
        switched.out
    );
    let read = ok(data, &["note", "get", "1"]);
    assert!(
        first(&read.out).ends_with("· no spell check"),
        "{}",
        read.out
    );

    let again = run(data, &["--json", "note", "spell-check", "1", "off"], None);
    assert_eq!(again.code, 4, "{} {}", again.out, again.err);
    assert_eq!(again.failure()["error"]["code"], "rejected");
}

#[test]
fn a_note_is_archived_and_brought_back_from_the_command_line() {
    let dir = scratch();
    let data = dir.path();
    ok(data, &["note", "create", "Mention to Anna"]);
    ok(data, &["note", "create", "Milk"]);

    let archived = ok(data, &["note", "archive", "1"]);
    assert!(
        archived.out.contains("Archived note 1."),
        "{}",
        archived.out
    );

    let listed = ok(data, &["note", "list"]);
    assert!(!listed.out.contains("Mention to Anna"), "{}", listed.out);
    let archive = ok(data, &["note", "list", "--archived"]);
    assert!(
        archive.out.contains("Mention to Anna · archived"),
        "{}",
        archive.out
    );
    assert!(!archive.out.contains("Milk"), "{}", archive.out);
    let read = ok(data, &["note", "get", "1"]);
    assert!(
        read.out
            .lines()
            .next()
            .unwrap_or_default()
            .contains("· archived"),
        "{}",
        read.out
    );

    // Archiving it again is the rules saying no.
    let again = run(data, &["--json", "note", "archive", "1"], None);
    assert_eq!(again.code, 4, "{} {}", again.out, again.err);
    assert_eq!(again.failure()["error"]["code"], "rejected");

    ok(data, &["note", "unarchive", "1"]);
    let listed = ok(data, &["--json", "note", "list"]).json();
    assert_eq!(listed["data"]["count"], 2);
    assert!(listed["data"]["notes"][1]["archived_at"].is_null());
}

// ---- text that is too big to be an argument -------------------------

#[test]
fn a_note_read_from_standard_input_keeps_every_newline_and_every_letter() {
    let dir = scratch();
    let data = dir.path();
    let body = "første linje\n\tanden linje ✓\n\nefter en tom linje 日本語\nemoji 🌱\n";

    let made = run(data, &["--json", "note", "create", "--stdin"], Some(body)).json();
    let id = made["data"]["note"]["id"].to_string();
    assert_eq!(made["data"]["note"]["body"], body);

    // And it is still that, byte for byte, once it has been through the
    // database and come back.
    let read = ok(data, &["--json", "note", "get", &id]).json();
    assert_eq!(read["data"]["note"]["body"], body);

    // The reading of it writes the body out as it is.
    let written = ok(data, &["note", "get", &id]);
    assert!(
        written.out.contains("efter en tom linje 日本語"),
        "{}",
        written.out
    );
    assert!(written.out.contains("emoji 🌱"), "{}", written.out);
}

#[test]
fn a_note_read_from_a_file_is_the_file() {
    let dir = scratch();
    let data = dir.path();
    let path = dir.path().join("note.txt");
    let body = "line one\nline two\n";
    std::fs::write(&path, body).expect("a file");

    let made = ok(
        data,
        &[
            "--json",
            "note",
            "create",
            "--file",
            path.to_str().expect("a path"),
        ],
    )
    .json();
    assert_eq!(made["data"]["note"]["body"], body);

    let replaced = run(
        data,
        &["--json", "note", "update", "1", "--stdin"],
        Some("all new\n"),
    )
    .json();
    assert_eq!(replaced["data"]["note"]["body"], "all new\n");
}

#[test]
fn a_file_that_is_not_there_is_said_rather_than_guessed_at() {
    let dir = scratch();
    let done = run(
        dir.path(),
        &["--json", "note", "create", "--file", "/nowhere/at/all"],
        None,
    );
    assert_eq!(done.code, 1, "{}", done.out);
    assert_eq!(done.failure()["error"]["code"], "input_error");
}

// ---- a JSON body ----------------------------------------------------

#[test]
fn a_json_body_supplies_the_fields_the_command_line_left_out() {
    let dir = scratch();
    let data = dir.path();
    let body = r#"{"title":"From the body","place":{"kind":"day","day":"today"},"focus":true}"#;
    let added = run(data, &["--json", "task", "add", "--input", "-"], Some(body)).json();
    assert_eq!(added["data"]["task"]["title"], "From the body");
    assert_eq!(added["data"]["task"]["focus"], true);

    let path = dir.path().join("body.json");
    std::fs::write(&path, r#"{"ids":[1],"place":{"kind":"backlog"}}"#).expect("a file");
    let moved = ok(
        data,
        &[
            "--json",
            "task",
            "move",
            "--input",
            path.to_str().expect("a path"),
        ],
    )
    .json();
    assert_eq!(moved["data"]["tasks"][0]["place"]["kind"], "backlog");
}

#[test]
fn a_body_cannot_quietly_be_another_operation_or_a_second_answer() {
    let dir = scratch();
    let data = dir.path();
    ok(data, &["task", "add", "One", "--today"]);

    // Pretending to be another operation.
    let spoofed = run(
        data,
        &["--json", "task", "add", "x", "--input", "-"],
        Some(r#"{"op":"task.delete","ids":[1]}"#),
    );
    assert_eq!(spoofed.code, 2, "{}", spoofed.out);
    assert!(
        spoofed.failure()["error"]["message"]
            .as_str()
            .expect("a message")
            .contains("task.delete")
    );

    // Saying the same field twice.
    let twice = run(
        data,
        &["--json", "task", "add", "Written out", "--input", "-"],
        Some(r#"{"title":"In the body"}"#),
    );
    assert_eq!(twice.code, 2);

    // A field nobody knows, which is the service's to refuse.
    let unknown = run(
        data,
        &["--json", "task", "add", "x", "--today", "--input", "-"],
        Some(r#"{"colour":"blue"}"#),
    );
    assert_eq!(unknown.code, 2);

    // The task that was there is still the only one.
    assert_eq!(
        ok(data, &["--json", "task", "list"]).json()["data"]["total"],
        1
    );
}

// ---- reading changes nothing ----------------------------------------

#[test]
fn reading_the_review_does_not_spend_it_and_starting_it_does() {
    let dir = scratch();
    let data = dir.path();
    ok(data, &["task", "add", "One", "--today"]);

    let before = ok(data, &["--json", "review", "get"]).json();
    assert_eq!(before["data"]["gate"]["started_today"], false);
    // Reading it twice is still reading it.
    let again = ok(data, &["--json", "review", "get"]).json();
    assert_eq!(again["data"]["gate"]["started_today"], false);

    let started = ok(data, &["--json", "review", "start"]).json();
    assert_eq!(started["data"]["gate"]["started_today"], true);
    assert!(
        started["data"]["undo"].is_null(),
        "starting a review is not undoable"
    );
    // And the gate stays where it is on the same day.
    assert_eq!(
        ok(data, &["--json", "review", "get"]).json()["data"]["gate"]["started_today"],
        true
    );
}

#[test]
fn recurrence_is_pending_until_a_refresh_rather_than_made_by_a_read() {
    let dir = scratch();
    let data = dir.path();
    ok(data, &["task", "add", "Standup", "--day", "2026-01-01"]);
    ok(data, &["schedule", "create", "1", "daily"]);

    // Reading says there is work without doing it, however often it is
    // read.
    let read = ok(data, &["--json", "day", "get"]).json();
    assert_eq!(read["context"]["recurrence_pending"], true);
    let before = ok(data, &["--json", "task", "list"]).json()["data"]["total"]
        .as_i64()
        .expect("a total");
    assert_eq!(
        ok(data, &["--json", "task", "list"]).json()["data"]["total"],
        before
    );

    let made = ok(data, &["--json", "refresh"]).json();
    assert!(
        made["data"]["count"].as_i64().expect("a count") > 0,
        "refresh should make the copies that are due"
    );
    assert!(made["data"]["undo"].is_null(), "generation is not undoable");
    assert_eq!(
        ok(data, &["--json", "refresh"]).json()["data"]["count"],
        0,
        "and there is nothing left to make"
    );
}

// ---- the database a command works on --------------------------------

#[test]
fn a_data_directory_is_the_whole_of_what_a_command_can_see() {
    let one = scratch();
    let other = scratch();
    ok(one.path(), &["task", "add", "Only here", "--today"]);
    assert_eq!(
        ok(one.path(), &["--json", "task", "list"]).json()["data"]["total"],
        1
    );
    assert_eq!(
        ok(other.path(), &["--json", "task", "list"]).json()["data"]["total"],
        0
    );
}

#[test]
fn nothing_on_the_command_line_still_means_the_app_rather_than_the_help() {
    // The app cannot be opened here, so what is checked is which road was
    // taken: a database that could not be opened, which is the launch
    // failing, rather than a usage error or a page of help.
    let dir = scratch();
    let done = run(&impossible(&dir), &[], None);
    assert_eq!(done.code, 1, "out: {} err: {}", done.out, done.err);
    assert!(done.out.is_empty(), "{}", done.out);
    assert!(done.err.contains("could not be made"), "{}", done.err);
}

#[test]
fn the_desktop_command_is_still_read_the_way_it_was() {
    // Only the forms that stop at the command line: the ones that go
    // further write a window rule, which is not a test's to write.
    let dir = scratch();
    for arguments in [
        vec!["desktop", "--floating", "--tiled"],
        vec!["desktop", "--size", "roomy"],
        vec!["desktop", "--size"],
        vec!["desktop", "--quickly"],
    ] {
        let done = run(dir.path(), &arguments, None);
        assert_eq!(done.code, 2, "{arguments:?} said {}", done.out);
    }
    let help = run(dir.path(), &["desktop", "--help"], None);
    assert_eq!(help.code, 0);
    assert!(help.out.contains("--floating"), "{}", help.out);
}

// ---- settings, the dictionary and the window ------------------------

#[test]
fn a_setting_outside_its_range_is_refused_rather_than_held_to_it() {
    let dir = scratch();
    let data = dir.path();
    let refused = run(
        data,
        &["--json", "settings", "set", "day_starts_at=48"],
        None,
    );
    assert_eq!(refused.code, 2, "{}", refused.out);
    assert_eq!(
        ok(data, &["--json", "settings", "get"]).json()["data"]["settings"]["day_starts_at"],
        5,
        "the setting is untouched"
    );
    let saved = ok(
        data,
        &["--json", "settings", "set", "day_starts_at=6", "mouse=off"],
    )
    .json();
    assert_eq!(saved["data"]["settings"]["day_starts_at"], 6);
    assert_eq!(saved["data"]["settings"]["mouse"], false);
    assert!(saved["data"]["undo"].is_null(), "a setting is not undoable");
}

#[test]
fn changing_the_window_says_to_write_the_rule_rather_than_writing_one() {
    let dir = scratch();
    let saved = ok(dir.path(), &["settings", "set", "window_size=900x700"]);
    assert!(saved.out.contains("jobsdone desktop"), "{}", saved.out);
    // Nothing here reached the window manager: it only said what to run.
}

#[test]
fn the_personal_dictionary_is_kept_by_the_word_it_was_written_as() {
    let dir = scratch();
    let data = dir.path();
    ok(data, &["dictionary", "add", "Acme"]);
    let listed = ok(data, &["--json", "dictionary", "list"]).json();
    assert_eq!(listed["data"]["words"][0]["word"], "Acme");
    ok(data, &["dictionary", "update", "acme", "ACME"]);
    assert_eq!(
        ok(data, &["--json", "dictionary", "list"]).json()["data"]["words"][0]["word"],
        "ACME"
    );
    ok(data, &["dictionary", "delete", "AcMe"]);
    assert_eq!(
        ok(data, &["--json", "dictionary", "list"]).json()["data"]["count"],
        0
    );
}

#[test]
fn a_popped_undo_identity_cannot_target_a_new_action() {
    let dir = TempDir::new().unwrap();
    let data = dir.path();
    let added = ok(data, &["--json", "task", "add", "A"]).json();
    let entry = added["data"]["undo"]["id"].as_i64().unwrap().to_string();
    ok(data, &["undo", "apply", "--entry", &entry]);
    let added = ok(data, &["--json", "task", "add", "B"]).json();
    let stale = run(data, &["--json", "undo", "apply", "--entry", &entry], None);
    assert_eq!(stale.code, 5, "{}", stale.err);
    let id = added["data"]["task"]["id"].as_i64().unwrap().to_string();
    assert_eq!(
        ok(data, &["--json", "task", "get", &id]).json()["data"]["task"]["title"],
        "B"
    );
}
