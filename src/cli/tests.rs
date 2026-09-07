//! What a command line means. Nothing here opens a database or reads a
//! file: `parse` is a pure function of the arguments, which is the whole
//! reason the grammar lives in its own module. What the built program does
//! with the answer is `tests/cli.rs`.

use serde_json::{Value, json};

use super::*;

fn read(arguments: &[&str]) -> Result<Parsed, Failure> {
    let arguments: Vec<String> = arguments.iter().map(|word| (*word).to_owned()).collect();
    parse(&arguments)
}

/// The request a command line sends.
fn request(arguments: &[&str]) -> Value {
    match read(arguments).expect("a command line that parses").plan {
        Plan::Operate(operation) => operation.request,
        other => panic!("{arguments:?} is {other:?} rather than an operation"),
    }
}

fn operation(arguments: &[&str]) -> Operation {
    match read(arguments).expect("a command line that parses").plan {
        Plan::Operate(operation) => operation,
        other => panic!("{arguments:?} is {other:?} rather than an operation"),
    }
}

fn refused(arguments: &[&str]) -> String {
    read(arguments)
        .expect_err(&format!("{arguments:?} should be refused"))
        .message
}

// ---- what a command line is at all ----------------------------------

#[test]
fn nothing_on_the_command_line_opens_the_app() {
    assert_eq!(read(&[]).unwrap().plan, Plan::Run);
}

#[test]
fn a_data_directory_opens_the_app_on_that_database() {
    let parsed = read(&["--data-dir", "/tmp/scratch"]).unwrap();
    assert_eq!(parsed.plan, Plan::Run);
    assert_eq!(parsed.data_dir, Some(PathBuf::from("/tmp/scratch")));
}

#[test]
fn an_option_about_an_operation_does_not_open_the_app() {
    // Opening the window because there was nothing to apply --json to
    // would be the one answer nobody asked for.
    for stray in [
        vec!["--json"],
        vec!["--format", "json"],
        vec!["--input", "-"],
    ] {
        let said = refused(&stray);
        assert!(
            said.contains("there is no command here"),
            "{stray:?} said {said}"
        );
    }
}

#[test]
fn a_word_that_is_not_a_command_is_refused_by_name() {
    assert_eq!(refused(&["fly"]), "fly is not a command");
    assert!(refused(&["task"]).contains("task wants one of"));
    assert!(refused(&["task", "frobnicate"]).contains("is not a command"));
}

#[test]
fn help_and_version_and_the_skill_need_no_command() {
    assert!(matches!(read(&["--help"]).unwrap().plan, Plan::Help(_)));
    assert!(matches!(read(&["-h"]).unwrap().plan, Plan::Help(_)));
    assert_eq!(read(&["--version"]).unwrap().plan, Plan::Version);
    assert_eq!(read(&["-V"]).unwrap().plan, Plan::Version);
    assert_eq!(read(&["--skill"]).unwrap().plan, Plan::Skill);
}

#[test]
fn help_after_a_command_is_that_command_s_help() {
    let Plan::Help(text) = read(&["task", "add", "--help"]).unwrap().plan else {
        panic!("not help");
    };
    assert!(text.contains("jobsdone task add TITLE"));

    let Plan::Help(text) = read(&["help", "note", "check"]).unwrap().plan else {
        panic!("not help");
    };
    assert!(text.contains("note check"));
}

#[test]
fn help_wins_over_a_command_line_that_makes_no_sense() {
    // `--help` on a half-typed command is somebody asking what the command
    // takes, which is exactly the moment to answer rather than complain.
    assert!(matches!(
        read(&["task", "add", "--help"]).unwrap().plan,
        Plan::Help(_)
    ));
}

// ---- the format -----------------------------------------------------

#[test]
fn the_format_is_asked_for_in_two_ways_that_must_agree() {
    assert_eq!(
        read(&["--json", "backlog", "get"]).unwrap().format,
        Format::Json
    );
    assert_eq!(
        read(&["backlog", "get", "--format", "json"])
            .unwrap()
            .format,
        Format::Json
    );
    assert_eq!(read(&["backlog", "get"]).unwrap().format, Format::Text);
    assert!(
        refused(&["backlog", "get", "--json", "--format", "text"])
            .contains("ask for different formats")
    );
}

#[test]
fn toon_is_refused_by_name() {
    let said = refused(&["backlog", "get", "--format", "toon"]);
    assert!(said.contains("toon"), "{said}");
    assert!(said.contains("text and json"), "{said}");
}

#[test]
fn the_format_survives_a_command_line_that_cannot_be_read() {
    // Whichever side of the mistake it was written on.
    let json = ["--json".to_owned(), "task".to_owned(), "fly".to_owned()];
    assert_eq!(parse::format_hint(&json), Format::Json);
    let after = ["task".to_owned(), "fly".to_owned(), "--json".to_owned()];
    assert_eq!(parse::format_hint(&after), Format::Json);
    assert!(parse(&json).is_err());
}

#[test]
fn a_global_option_stands_anywhere() {
    for line in [
        vec!["--json", "task", "get", "1"],
        vec!["task", "--json", "get", "1"],
        vec!["task", "get", "--json", "1"],
        vec!["task", "get", "1", "--json"],
    ] {
        let parsed = read(&line).unwrap();
        assert_eq!(parsed.format, Format::Json, "{line:?}");
        let Plan::Operate(operation) = parsed.plan else {
            panic!("{line:?}")
        };
        assert_eq!(operation.request, json!({"op": "task.get", "id": 1}));
    }
}

// ---- the names of things --------------------------------------------

#[test]
fn a_command_s_name_is_the_operation_it_sends() {
    for (command, op) in [
        (vec!["task", "list"], "task.list"),
        (vec!["day", "get"], "day.get"),
        (vec!["backlog", "get"], "backlog.get"),
        (vec!["history", "list"], "history.list"),
        (vec!["review", "get"], "review.get"),
        (vec!["review", "start"], "review.start"),
        (vec!["refresh"], "refresh"),
        (vec!["schedule", "list"], "schedule.list"),
        (vec!["note", "list"], "note.list"),
        (vec!["settings", "get"], "settings.get"),
        (vec!["dictionary", "list"], "dictionary.list"),
        (vec!["undo", "get"], "undo.get"),
    ] {
        assert_eq!(request(&command)["op"], op, "{command:?}");
    }
}

/// One command line per command in the table, so that every form the help
/// advertises is a form the parser takes. A command added without a line
/// here fails the test below rather than shipping unparsed.
const SAMPLES: &[(&str, &[&str])] = &[
    ("task list", &["task", "list"]),
    ("task get", &["task", "get", "1"]),
    ("task add", &["task", "add", "A title"]),
    ("task update", &["task", "update", "1", "--focus"]),
    ("task rename", &["task", "rename", "1", "A title"]),
    ("task focus", &["task", "focus", "1"]),
    ("task unfocus", &["task", "unfocus", "1"]),
    ("task waiting", &["task", "waiting", "1"]),
    ("task unwait", &["task", "unwait", "1"]),
    ("task due", &["task", "due", "1", "today"]),
    ("task remind", &["task", "remind", "1", "today"]),
    ("task close", &["task", "close", "1"]),
    ("task reopen", &["task", "reopen", "1"]),
    ("task move", &["task", "move", "1", "--to", "today"]),
    ("task reorder", &["task", "reorder", "1", "--top"]),
    ("task delete", &["task", "delete", "1", "--yes"]),
    ("day get", &["day", "get"]),
    ("day reorder", &["day", "reorder", "--order", "1,2"]),
    ("backlog get", &["backlog", "get"]),
    ("backlog reorder", &["backlog", "reorder", "--order", "1,2"]),
    ("history list", &["history", "list"]),
    ("search", &["search", "report"]),
    ("review get", &["review", "get"]),
    ("review start", &["review", "start"]),
    ("refresh", &["refresh"]),
    ("schedule list", &["schedule", "list"]),
    ("schedule get", &["schedule", "get", "1"]),
    ("schedule create", &["schedule", "create", "1", "daily"]),
    (
        "schedule update",
        &["schedule", "update", "1", "--rule", "daily"],
    ),
    ("schedule stop", &["schedule", "stop", "1"]),
    ("schedule preview", &["schedule", "preview", "daily"]),
    ("note list", &["note", "list"]),
    ("note get", &["note", "get", "1"]),
    ("note create", &["note", "create"]),
    ("note update", &["note", "update", "1", "a thought"]),
    ("note delete", &["note", "delete", "1", "--yes"]),
    ("note check", &["note", "check", "1"]),
    ("note copy", &["note", "copy", "1"]),
    ("settings get", &["settings", "get"]),
    ("settings set", &["settings", "set", "mouse=off"]),
    ("dictionary list", &["dictionary", "list"]),
    ("dictionary add", &["dictionary", "add", "Acme"]),
    (
        "dictionary update",
        &["dictionary", "update", "acme", "ACME"],
    ),
    ("dictionary delete", &["dictionary", "delete", "acme"]),
    ("undo get", &["undo", "get"]),
    ("undo apply", &["undo", "apply", "--entry", "1"]),
    ("desktop", &["desktop", "--tiled"]),
    ("help", &["help"]),
];

#[test]
fn every_command_in_the_table_parses_and_sends_its_own_operation() {
    for spec in parse::all_commands() {
        let (_, arguments) = SAMPLES
            .iter()
            .find(|(name, _)| *name == spec.name)
            .unwrap_or_else(|| panic!("{} has no sample command line", spec.name));

        let parsed = read(arguments)
            .unwrap_or_else(|failure| panic!("{arguments:?} is refused: {}", failure.message));
        if spec.op.is_empty() {
            continue;
        }
        let Plan::Operate(operation) = parsed.plan else {
            panic!("{arguments:?} is not an operation");
        };
        assert_eq!(operation.request["op"], spec.op, "{arguments:?}");
    }
}

/// The command's own words, and nothing else, are the command: a verb that
/// happens to be a whole command's name on its own does not shadow it.
#[test]
fn a_verb_is_not_eaten_by_a_noun_that_is_a_command_by_itself() {
    assert_eq!(request(&["history", "list"])["op"], "history.list");
    assert!(read(&["history", "list", "--limit", "3"]).is_ok());
    assert_eq!(request(&["history"])["op"], "history.list");
    // `help` names what it is about rather than taking a verb of its own.
    let Plan::Help(text) = read(&["help", "day", "get"]).unwrap().plan else {
        panic!("not help")
    };
    assert!(text.contains("jobsdone day get"), "{text}");
}

#[test]
fn the_aliases_reach_the_same_operations() {
    assert_eq!(request(&["tasks", "ls"])["op"], "task.list");
    assert_eq!(request(&["task", "show", "3"])["op"], "task.get");
    assert_eq!(
        request(&["task", "edit", "3", "--focus"])["op"],
        "task.update"
    );
    assert_eq!(request(&["notes", "ls"])["op"], "note.list");
    assert_eq!(request(&["dict", "ls"])["op"], "dictionary.list");
    assert_eq!(request(&["dict", "rm", "acme"])["op"], "dictionary.delete");
    assert_eq!(request(&["day", "list"])["op"], "history.list");
    assert_eq!(request(&["history"])["op"], "history.list");
    assert_eq!(request(&["undo", "show"])["op"], "undo.get");
}

#[test]
fn an_unknown_option_is_named_against_the_command_it_was_written_on() {
    assert_eq!(
        refused(&["task", "add", "x", "--quickly"]),
        "--quickly is not an option of `task add`"
    );
    assert_eq!(refused(&["--colour"]), "--colour is not an option");
    // An option of another command is still not this one's.
    assert!(refused(&["task", "list", "--yes"]).contains("`task list`"));
}

// ---- tasks ----------------------------------------------------------

#[test]
fn a_task_goes_to_the_backlog_unless_a_day_is_named() {
    assert_eq!(
        request(&["task", "add", "Fix the tap"]),
        json!({"op": "task.add", "title": "Fix the tap", "place": {"kind": "backlog"}})
    );
    assert_eq!(
        request(&["task", "add", "Report", "--today"])["place"],
        json!({"kind": "day", "day": "today"})
    );
    assert_eq!(
        request(&["task", "add", "Report", "--day", "2026-09-30"])["place"],
        json!({"kind": "day", "day": "2026-09-30"})
    );
    assert_eq!(
        request(&["task", "add", "Report", "--place", "backlog"])["place"],
        json!({"kind": "backlog"})
    );
}

#[test]
fn a_title_written_without_quotes_still_arrives_as_one_title() {
    assert_eq!(
        request(&["task", "add", "Write", "the", "report"])["title"],
        "Write the report"
    );
    assert_eq!(request(&["search", "the", "report"])["text"], "the report");
}

#[test]
fn everything_a_new_task_starts_with_goes_in_one_request() {
    assert_eq!(
        request(&[
            "task",
            "add",
            "Report",
            "--today",
            "--focus",
            "--due",
            "tomorrow",
            "--remind",
            "2026-09-09",
            "--repeat",
            "weekly:mon,thu",
            "--position",
            "2",
        ]),
        json!({
            "op": "task.add",
            "title": "Report",
            "place": {"kind": "day", "day": "today"},
            "focus": true,
            "due": "tomorrow",
            "remind": "2026-09-09",
            "repeat": {"kind": "weekly", "weekdays": ["mon", "thu"]},
            "position": 2,
        })
    );
}

#[test]
fn two_places_for_one_task_is_refused_rather_than_resolved() {
    assert!(
        refused(&["task", "add", "x", "--today", "--backlog"]).contains("name different places")
    );
    // The same place twice is nobody contradicting themselves.
    assert!(read(&["task", "add", "x", "--today", "--today"]).is_ok());
}

#[test]
fn a_date_is_passed_through_for_the_service_to_read() {
    // The service owns what a typed date means, so the adapter neither
    // resolves one nor refuses one it does not recognise.
    for written in ["2026-09-30", "today", "tomorrow", "next-work-day"] {
        assert_eq!(
            request(&["task", "due", "3", written])["due"],
            Value::String(written.to_owned())
        );
    }
}

#[test]
fn clearing_a_date_is_null_and_leaving_it_is_absent() {
    assert_eq!(
        request(&["task", "update", "3", "--clear-due"]),
        json!({"op": "task.update", "id": 3, "due": null})
    );
    assert_eq!(request(&["task", "due", "3", "none"])["due"], Value::Null);
    assert_eq!(request(&["task", "due", "3", "clear"])["due"], Value::Null);
    let leaving = request(&["task", "update", "3", "--focus"]);
    assert!(leaving.get("due").is_none());
}

#[test]
fn a_property_and_its_opposite_cannot_both_be_asked_for() {
    assert!(refused(&["task", "update", "3", "--focus", "--no-focus"]).contains("opposites"));
    assert!(
        refused(&["task", "update", "3", "--due", "today", "--clear-due"]).contains("opposites")
    );
}

#[test]
fn the_short_ways_of_changing_a_task_are_one_update_each() {
    assert_eq!(
        request(&["task", "focus", "3"]),
        json!({"op": "task.update", "id": 3, "focus": true})
    );
    assert_eq!(
        request(&["task", "unfocus", "3"]),
        json!({"op": "task.update", "id": 3, "focus": false})
    );
    assert_eq!(
        request(&["task", "waiting", "3"]),
        json!({"op": "task.update", "id": 3, "waiting": true})
    );
    assert_eq!(
        request(&["task", "unwait", "3"]),
        json!({"op": "task.update", "id": 3, "waiting": false})
    );
    assert_eq!(
        request(&["task", "rename", "3", "A", "new", "title"]),
        json!({"op": "task.update", "id": 3, "title": "A new title"})
    );
    assert_eq!(
        request(&["task", "rename", "3", "Standup", "--scope", "future"])["title_scope"],
        "future"
    );
    assert!(refused(&["task", "rename", "3", "x", "--scope", "later"]).contains("not a scope"));
}

#[test]
fn several_tasks_move_in_one_request() {
    // Whether they were written apart or together: a move of five tasks is
    // one invocation, never five.
    let by_space = request(&["task", "move", "3", "4", "5", "--to", "backlog"]);
    let by_comma = request(&["task", "move", "3,4,5", "--to", "backlog"]);
    assert_eq!(by_space, by_comma);
    assert_eq!(
        by_space,
        json!({"op": "task.move", "ids": [3, 4, 5], "place": {"kind": "backlog"}})
    );
    assert_eq!(request(&["task", "close", "1", "2"])["ids"], json!([1, 2]));
    assert_eq!(request(&["task", "delete", "9"])["ids"], json!([9]));
}

#[test]
fn an_id_given_twice_is_refused() {
    assert_eq!(refused(&["task", "close", "3", "3"]), "3 is given twice");
    assert_eq!(refused(&["task", "close", "3,4,3"]), "3 is given twice");
}

#[test]
fn a_deletion_is_confirmed_only_where_it_was_asked_to_be() {
    assert!(request(&["task", "delete", "9"]).get("confirm").is_none());
    assert_eq!(request(&["task", "delete", "9", "--yes"])["confirm"], true);
    assert_eq!(request(&["note", "delete", "4", "--yes"])["confirm"], true);
}

#[test]
fn a_task_is_reordered_one_way_at_a_time() {
    assert_eq!(
        request(&["task", "reorder", "3", "--position", "2"]),
        json!({"op": "task.reorder", "id": 3, "position": 2})
    );
    assert_eq!(request(&["task", "reorder", "3", "--top"])["position"], 1);
    assert_eq!(
        request(&["task", "reorder", "3", "--before", "7"])["before"],
        7
    );
    assert_eq!(
        request(&["task", "reorder", "3", "--after", "7"])["after"],
        7
    );
    assert!(refused(&["task", "reorder", "3", "--top", "--after", "7"]).contains("use one"));
    assert!(refused(&["task", "reorder", "3"]).contains("--position"));
}

#[test]
fn a_whole_order_is_one_request_for_a_day_or_for_the_backlog() {
    assert_eq!(
        request(&["day", "reorder", "2026-09-08", "--order", "3,1,2"]),
        json!({
            "op": "day.reorder",
            "place": {"kind": "day", "day": "2026-09-08"},
            "ids": [3, 1, 2],
        })
    );
    assert_eq!(
        request(&["day", "reorder", "--order", "3,1"])["place"],
        json!({"kind": "day", "day": "today"})
    );
    assert_eq!(
        request(&["backlog", "reorder", "--order", "9,8"]),
        json!({"op": "day.reorder", "place": {"kind": "backlog"}, "ids": [9, 8]})
    );
    assert!(refused(&["day", "reorder", "--order", "3,3"]).contains("given twice"));
    assert!(refused(&["backlog", "reorder"]).contains("--order"));
}

// ---- schedules ------------------------------------------------------

#[test]
fn the_five_shapes_of_a_rule_are_written_shortly() {
    for (written, rule) in [
        ("workdays", json!({"kind": "workdays"})),
        ("daily", json!({"kind": "daily"})),
        (
            "weekly:mon,thu",
            json!({"kind": "weekly", "weekdays": ["mon", "thu"]}),
        ),
        ("monthly:15", json!({"kind": "monthly", "day": 15})),
        ("monthly:last", json!({"kind": "monthly", "day": "last"})),
        (
            "every:2w:2026-09-15",
            json!({"kind": "every_n_weeks", "n": 2, "from": "2026-09-15"}),
        ),
    ] {
        assert_eq!(
            request(&["schedule", "create", "4", written])["rule"],
            rule,
            "{written}"
        );
    }
}

#[test]
fn a_rule_nobody_can_read_names_the_ones_that_can_be() {
    let said = refused(&["schedule", "create", "4", "sometimes"]);
    assert!(said.contains("workdays"), "{said}");
    assert!(said.contains("every:2w:DATE"), "{said}");
    assert!(refused(&["schedule", "create", "4", "weekly:"]).contains("weekdays"));
    assert!(refused(&["schedule", "create", "4", "every:2w"]).contains("count from"));
}

#[test]
fn a_schedule_is_previewed_from_a_rule_or_from_itself_but_not_both() {
    assert_eq!(
        request(&["schedule", "preview", "daily", "--count", "5"]),
        json!({"op": "schedule.preview", "rule": {"kind": "daily"}, "count": 5})
    );
    assert_eq!(
        request(&["schedule", "preview", "--schedule", "3"])["schedule"],
        3
    );
    assert!(refused(&["schedule", "preview", "daily", "--schedule", "3"]).contains("not both"));
}

#[test]
fn a_schedule_changes_its_rule_its_title_or_both() {
    assert_eq!(
        request(&[
            "schedule", "update", "3", "--rule", "daily", "--title", "Standup"
        ]),
        json!({
            "op": "schedule.update",
            "id": 3,
            "rule": {"kind": "daily"},
            "title": "Standup",
        })
    );
}

// ---- notes ----------------------------------------------------------

#[test]
fn a_note_s_text_comes_from_one_place() {
    assert_eq!(
        request(&["note", "create", "a thought"]),
        json!({"op": "note.create", "body": "a thought"})
    );
    // Nothing at all is an empty note, which is a real thing to make.
    assert_eq!(request(&["note", "create"])["body"], "");
    assert!(refused(&["note", "create", "a thought", "--stdin"]).contains("one place"));
    assert!(refused(&["note", "update", "4"]).contains("--stdin"));
}

#[test]
fn text_that_must_be_read_is_left_for_somebody_who_can_read_it() {
    let reading = operation(&["note", "update", "4", "--stdin"]);
    assert_eq!(
        reading.body,
        Some(Body {
            field: "body",
            source: Source::Stdin
        })
    );
    assert!(reading.request.get("body").is_none());

    let from_a_file = operation(&["note", "create", "--file", "/tmp/note.txt"]);
    assert_eq!(
        from_a_file.body,
        Some(Body {
            field: "body",
            source: Source::File(PathBuf::from("/tmp/note.txt"))
        })
    );
}

#[test]
fn a_check_is_of_a_note_or_of_some_text() {
    assert_eq!(
        request(&["note", "check", "4", "--suggestions"]),
        json!({"op": "note.check", "note": 4, "suggestions": true})
    );
    assert_eq!(
        request(&["note", "check", "--text", "teh cat"])["text"],
        "teh cat"
    );
    assert_eq!(
        operation(&["note", "check", "--stdin"]).body.unwrap().field,
        "text"
    );
    assert!(refused(&["note", "check", "4", "--text", "x"]).contains("not both"));
    assert!(refused(&["note", "check"]).contains("a note id"));
}

#[test]
fn copying_a_note_reads_it_and_then_reaches_the_clipboard() {
    let operation = operation(&["note", "copy", "4"]);
    assert_eq!(operation.request, json!({"op": "note.get", "id": 4}));
    assert_eq!(operation.then, Then::CopyNote);
    assert_eq!(operation.view, render::View::NoteCopy);
}

// ---- settings and the dictionary ------------------------------------

#[test]
fn a_setting_is_sent_as_the_shape_it_holds() {
    assert_eq!(
        request(&[
            "settings",
            "set",
            "day_starts_at=6",
            "work_days=mon,tue",
            "window_size=900x700",
            "mouse=off",
            "date_style=day_first",
        ])["settings"],
        json!({
            "day_starts_at": 6,
            "work_days": ["mon", "tue"],
            "window_size": {"width": 900, "height": 700},
            "mouse": false,
            "date_style": "day_first",
        })
    );
    for written in ["true", "on", "yes", "1"] {
        assert_eq!(
            request(&["settings", "set", &format!("mouse={written}")])["settings"]["mouse"],
            true
        );
    }
}

#[test]
fn a_setting_nobody_has_names_the_ones_there_are() {
    let said = refused(&["settings", "set", "colour=blue"]);
    assert!(said.contains("colour is not a setting"), "{said}");
    assert!(said.contains("day_starts_at"), "{said}");
    assert!(refused(&["settings", "set", "mouse"]).contains("not a KEY=VALUE"));
    assert!(refused(&["settings", "set", "mouse=maybe"]).contains("on or off"));
    assert!(refused(&["settings", "set", "day_starts_at=six"]).contains("whole number"));
    assert!(refused(&["settings", "set", "mouse=on", "mouse=off"]).contains("set twice"));
}

#[test]
fn a_dictionary_entry_is_found_by_the_word_it_was_written_as() {
    assert_eq!(
        request(&["dictionary", "add", "Acme"]),
        json!({"op": "dictionary.add", "word": "Acme"})
    );
    assert_eq!(
        request(&["dictionary", "update", "acme", "ACME"]),
        json!({"op": "dictionary.update", "key": "acme", "word": "ACME"})
    );
    assert_eq!(
        request(&["dictionary", "delete", "acme"]),
        json!({"op": "dictionary.delete", "key": "acme"})
    );
}

#[test]
fn undo_is_guarded_by_the_entry_it_was_told_about() {
    assert_eq!(request(&["undo", "apply"]), json!({"op": "undo.apply"}));
    assert_eq!(
        request(&["undo", "apply", "--entry", "42"])["expected_id"],
        42
    );
    assert!(refused(&["undo", "apply", "--entry", "0"]).contains("not an id"));
}

// ---- a JSON body ----------------------------------------------------

fn merged(arguments: &[&str], body: &str) -> Result<Value, Failure> {
    let mut operation = operation(arguments);
    complete(&mut operation, None, Some(body.to_owned()))?;
    Ok(operation.request)
}

#[test]
fn a_body_supplies_the_fields_the_command_line_left_out() {
    assert_eq!(
        merged(
            &["task", "add", "--input", "-"],
            r#"{"title": "From the body", "place": {"kind": "backlog"}}"#
        )
        .unwrap(),
        json!({
            "op": "task.add",
            "title": "From the body",
            "place": {"kind": "backlog"},
        })
    );
}

#[test]
fn a_body_may_name_the_operation_it_is_for_but_not_another_one() {
    assert!(
        merged(
            &["task", "add", "x", "--input", "-"],
            r#"{"op": "task.add"}"#
        )
        .is_ok()
    );
    let said = merged(
        &["task", "add", "x", "--input", "-"],
        r#"{"op": "task.delete", "ids": [1]}"#,
    )
    .unwrap_err()
    .message;
    assert!(said.contains("task.delete"), "{said}");
    assert!(said.contains("task.add"), "{said}");
}

#[test]
fn a_field_set_twice_is_refused_rather_than_one_of_them_winning() {
    let said = merged(
        &["task", "add", "Written out", "--input", "-"],
        r#"{"title": "In the body"}"#,
    )
    .unwrap_err()
    .message;
    assert_eq!(said, "title is given on the command line and in the input");

    // A place the command line named is a decision, so a body naming one
    // too is the same two answers to one question.
    let said = merged(
        &["task", "add", "x", "--today", "--input", "-"],
        r#"{"place": {"kind": "backlog"}}"#,
    )
    .unwrap_err()
    .message;
    assert!(said.contains("place"), "{said}");

    // With nothing named, the default is left out so the body can say.
    assert_eq!(
        merged(
            &["task", "add", "x", "--input", "-"],
            r#"{"place": {"kind": "backlog"}}"#,
        )
        .unwrap()["place"],
        json!({"kind": "backlog"})
    );
}

#[test]
fn a_body_that_is_not_an_object_of_fields_is_refused() {
    assert!(
        merged(&["task", "add", "x", "--input", "-"], "[1, 2]")
            .unwrap_err()
            .message
            .contains("JSON object")
    );
    assert!(
        merged(&["task", "add", "x", "--input", "-"], "not json")
            .unwrap_err()
            .message
            .contains("not JSON")
    );
}

#[test]
fn a_body_is_read_from_a_file_or_from_standard_input() {
    assert_eq!(
        operation(&["task", "add", "x", "--input", "-"]).input,
        Some(Source::Stdin)
    );
    assert_eq!(
        operation(&["task", "add", "x", "--input", "body.json"]).input,
        Some(Source::File(PathBuf::from("body.json")))
    );
}

#[test]
fn text_read_in_and_text_written_out_are_still_one_field() {
    let mut reading = operation(&["note", "create", "--stdin"]);
    complete(&mut reading, Some("a thought\n".to_owned()), None).unwrap();
    assert_eq!(reading.request["body"], "a thought\n");

    let mut both = operation(&["note", "create", "--stdin"]);
    let said = complete(
        &mut both,
        Some("read in".to_owned()),
        Some(r#"{"body": "in the body"}"#.to_owned()),
    )
    .unwrap_err()
    .message;
    assert!(said.contains("body"), "{said}");
}

// ---- what the desktop command was, and still is ---------------------

#[test]
fn the_desktop_command_carries_the_window_the_flags_asked_for() {
    assert_eq!(
        read(&["desktop"]).unwrap().plan,
        Plan::Desktop {
            floating: None,
            size: None
        }
    );
    assert_eq!(
        read(&["desktop", "--tiled"]).unwrap().plan,
        Plan::Desktop {
            floating: Some(false),
            size: None
        }
    );
    assert_eq!(
        read(&["desktop", "--floating", "--size", "1000x700"])
            .unwrap()
            .plan,
        Plan::Desktop {
            floating: Some(true),
            size: Some((1000, 700))
        }
    );
}

#[test]
fn a_floating_window_and_a_tiled_one_cannot_both_be_asked_for() {
    assert!(read(&["desktop", "--floating", "--tiled"]).is_err());
    assert!(read(&["desktop", "--tiled", "--tiled"]).is_ok());
}

#[test]
fn a_size_that_is_not_one_is_refused() {
    assert!(read(&["desktop", "--size", "roomy"]).is_err());
    assert!(read(&["desktop", "--size", "1000"]).is_err());
    assert!(read(&["desktop", "--size"]).is_err());
}

// ---- the shape of an answer -----------------------------------------

#[test]
fn a_failure_is_written_the_way_the_command_line_asked() {
    let failure = Failure::usage("fly is not a command");
    assert_eq!(
        report(&failure, Format::Text),
        "jobsdone: fly is not a command\n"
    );
    let json: Value = serde_json::from_str(&report(&failure, Format::Json)).unwrap();
    assert_eq!(
        json,
        json!({
            "schema_version": 1,
            "ok": false,
            "error": {"code": "usage", "message": "fly is not a command"},
        })
    );
}

#[test]
fn the_machine_response_is_the_service_s_own_envelope() {
    // The adapter adds nothing to it, so a reader parsing the response is
    // parsing what the service said.
    let envelope = json!({
        "schema_version": 1,
        "ok": true,
        "data": {"tasks": [], "total": 0},
        "context": {"today": "2026-09-07", "recurrence_pending": false},
    });
    let operation = operation(&["task", "list"]);
    let written = present(&operation, &envelope, Format::Json, DateOrder::DayFirst).unwrap();
    assert_eq!(serde_json::from_str::<Value>(&written).unwrap(), envelope);
    assert!(written.ends_with('\n'));
}

#[test]
fn a_reading_of_an_answer_is_a_list_rather_than_the_envelope() {
    let envelope = json!({
        "schema_version": 1,
        "ok": true,
        "data": {
            "tasks": [{
                "id": 12,
                "title": "Write the report",
                "place": {"kind": "day", "day": "2026-09-07"},
                "position": 1,
                "open": true,
                "focus": true,
                "waiting": false,
                "closed_at": null,
                "due_on": "2026-09-10",
                "remind_on": null,
                "schedule": null,
            }],
            "total": 1,
        },
        "context": {"today": "2026-09-07", "recurrence_pending": false},
    });
    let operation = operation(&["task", "list"]);
    let written = present(&operation, &envelope, Format::Text, DateOrder::DayFirst).unwrap();
    assert_eq!(
        written,
        "Mon 7 Sep (today)\n    12  [*] Write the report · due Thu 10 Sep\n\n1 task\n"
    );
    assert!(!written.contains('{'));
    assert!(!written.contains("\u{1b}"), "no escape sequences");
}

#[test]
fn a_mutation_says_what_it_did_and_how_to_take_it_back() {
    let envelope = json!({
        "schema_version": 1,
        "ok": true,
        "data": {
            "tasks": [
                {"id": 1, "title": "One", "place": {"kind": "backlog"}, "open": false,
                 "closed_at": "2026-09-07T14:35:00+02:00[Europe/Copenhagen]"},
                {"id": 2, "title": "Two", "place": {"kind": "backlog"}, "open": false,
                 "closed_at": "2026-09-07T14:35:00+02:00[Europe/Copenhagen]"},
            ],
            "count": 2,
            "undo": {"id": 4, "label": "Closed \"One\" and one more change"},
        },
        "context": {"today": "2026-09-07", "recurrence_pending": false},
    });
    let operation = operation(&["task", "close", "1", "2"]);
    let written = present(&operation, &envelope, Format::Text, DateOrder::DayFirst).unwrap();
    assert!(written.starts_with("Closed 2 tasks.\n"), "{written}");
    assert!(
        written.contains("jobsdone undo apply --entry 4"),
        "{written}"
    );
}

#[test]
fn changing_the_window_settings_says_to_write_the_rule_rather_than_writing_it() {
    let envelope = json!({
        "schema_version": 1,
        "ok": true,
        "data": {
            "settings": {"floating_window": true},
            "date_order": "day_first",
            "desktop": {"window_rule_changed": true},
            "undo": null,
        },
        "context": {"today": "2026-09-07", "recurrence_pending": false},
    });
    let operation = operation(&["settings", "set", "floating_window=on"]);
    let written = present(&operation, &envelope, Format::Text, DateOrder::DayFirst).unwrap();
    assert!(written.contains("jobsdone desktop"), "{written}");
    assert!(written.contains("Saved."), "{written}");
}

#[test]
fn the_bundled_skill_is_markdown_with_its_front_matter() {
    assert!(SKILL.starts_with("---\n"));
    assert!(SKILL.contains("name: jobsdone"));
    assert!(SKILL.contains("# Jobsdone"));
}

#[test]
fn the_help_lists_every_command_the_program_takes() {
    let Plan::Help(text) = read(&["--help"]).unwrap().plan else {
        panic!("not help")
    };
    for spec in parse::all_commands() {
        assert!(text.contains(spec.name), "{} is not in the help", spec.name);
    }
    assert!(
        text.contains("--skill"),
        "the help should say --skill is there"
    );
    assert!(text.contains("--input"));
    assert!(text.contains("--data-dir"));
}

#[test]
fn every_command_s_help_says_the_operation_it_sends() {
    for spec in parse::all_commands() {
        let words: Vec<&str> = spec.name.split(' ').chain(["--help"]).collect();
        let Plan::Help(text) = read(&words).unwrap().plan else {
            panic!("{} has no help", spec.name)
        };
        assert!(text.contains(spec.usage), "{} has no usage line", spec.name);
        if !spec.op.is_empty() {
            assert!(text.contains(spec.op), "{} does not name its op", spec.name);
        }
    }
}
