use super::*;

use serde_json::json;

use crate::domain::tests::MemStore;
use crate::domain::{Id, Place};

/// A store and a clock, driven the way the command line drives them: one
/// request at a time, each loading the model afresh.
struct World {
    store: MemStore,
    now: Zoned,
    dates: DateOrder,
}

impl World {
    /// 09:00 on Monday 7 September 2026. The zone is fixed so that no
    /// test depends on the machine it runs on.
    fn new() -> World {
        World {
            store: MemStore::new(),
            now: at("2026-09-07T09:00:00"),
            dates: DateOrder::DayFirst,
        }
    }

    fn clock(&mut self, now: &str) {
        self.now = at(now);
    }

    fn call(&mut self, request: Value) -> Result<Value, Error> {
        execute(&mut self.store, request, &self.now, self.dates)
    }

    /// The `data` of a request that was answered.
    fn ok(&mut self, request: Value) -> Value {
        match self.call(request.clone()) {
            Ok(response) => {
                assert_eq!(response["schema_version"], 1);
                assert_eq!(response["ok"], true);
                response["data"].clone()
            }
            Err(error) => panic!("{request} was refused: {} {}", error.code, error.message),
        }
    }

    /// The whole envelope, for the tests that are about the context.
    fn envelope(&mut self, request: Value) -> Value {
        self.call(request).expect("an answered request")
    }

    fn err(&mut self, request: Value) -> Error {
        match self.call(request.clone()) {
            Ok(response) => panic!("{request} was answered with {response}"),
            Err(error) => error,
        }
    }

    fn model(&self) -> Model {
        self.store.load().expect("a loaded model")
    }

    /// A task on today, by title, answering with its id.
    fn task(&mut self, title: &str) -> Id {
        let data = self.ok(json!({
            "op": "task.add",
            "title": title,
            "place": {"kind": "day", "day": "today"},
        }));
        data["task"]["id"].as_i64().expect("an id")
    }

    fn backlog_task(&mut self, title: &str) -> Id {
        let data = self.ok(json!({
            "op": "task.add",
            "title": title,
            "place": {"kind": "backlog"},
        }));
        data["task"]["id"].as_i64().expect("an id")
    }
}

fn at(text: &str) -> Zoned {
    format!("{text}+02:00[Europe/Copenhagen]")
        .parse()
        .expect("a zoned timestamp")
}

/// The ids of an array of tasks or rows, in the order they came back.
fn ids(value: &Value) -> Vec<i64> {
    value
        .as_array()
        .expect("an array")
        .iter()
        .map(|row| row["id"].as_i64().expect("an id"))
        .collect()
}

// ---- the envelope and the request ------------------------------------

#[test]
fn every_response_says_the_day_the_order_and_whether_copies_are_owed() {
    let mut world = World::new();
    let response = world.envelope(json!({"op": "backlog.get"}));

    assert_eq!(response["context"]["today"], "2026-09-07");
    assert_eq!(response["context"]["date_order"], "day_first");
    assert_eq!(response["context"]["recurrence_pending"], false);
}

#[test]
fn the_day_is_the_working_day_rather_than_the_calendar_one() {
    let mut world = World::new();
    world.clock("2026-09-08T01:30:00");
    let response = world.envelope(json!({"op": "backlog.get"}));

    assert_eq!(response["context"]["today"], "2026-09-07");
}

#[test]
fn a_request_that_is_not_one_says_so() {
    let mut world = World::new();

    assert_eq!(world.err(json!([1, 2])).code, "invalid_request");
    assert_eq!(world.err(json!({})).code, "invalid_request");
    assert_eq!(world.err(json!({"op": 3})).code, "invalid_request");

    let unknown = world.err(json!({"op": "task.explode"}));
    assert_eq!(unknown.code, "invalid_request");
    assert_eq!(unknown.exit_code, 2);
}

#[test]
fn a_field_the_operation_does_not_have_is_refused() {
    let mut world = World::new();
    let error = world.err(json!({
        "op": "task.add",
        "title": "Write the report",
        "place": {"kind": "backlog"},
        "focussed": true,
    }));

    assert_eq!(error.code, "invalid_request");
    assert!(error.message.contains("focussed"), "{}", error.message);
}

#[test]
fn a_place_with_something_extra_in_it_is_refused() {
    let mut world = World::new();
    let error = world.err(json!({
        "op": "task.add",
        "title": "Write the report",
        "place": {"kind": "day", "day": "today", "pane": "left"},
    }));

    assert_eq!(error.code, "invalid_argument");
    assert!(error.message.contains("pane"), "{}", error.message);
}

// ---- dates -----------------------------------------------------------

#[test]
fn the_four_words_a_date_may_be_written_as() {
    let mut world = World::new();
    let day = |world: &mut World, text: &str| -> String {
        let data = world.ok(json!({"op": "day.get", "day": text}));
        data["day"].as_str().expect("a day").to_owned()
    };

    assert_eq!(day(&mut world, "today"), "2026-09-07");
    assert_eq!(day(&mut world, "yesterday"), "2026-09-06");
    assert_eq!(day(&mut world, "tomorrow"), "2026-09-08");
    assert_eq!(day(&mut world, "2026-12-24"), "2026-12-24");
    // Monday, so the next work day is Tuesday.
    assert_eq!(day(&mut world, "next-work-day"), "2026-09-08");
    assert_eq!(day(&mut world, "TODAY"), "2026-09-07");
}

#[test]
fn next_work_day_follows_the_work_days_setting() {
    let mut world = World::new();
    world.ok(json!({"op": "settings.set", "settings": {"work_days": ["wed"]}}));
    let data = world.ok(json!({"op": "day.get", "day": "next-work-day"}));

    assert_eq!(data["day"], "2026-09-09");
}

#[test]
fn a_date_that_is_not_one_says_what_a_date_looks_like() {
    let mut world = World::new();
    let error = world.err(json!({"op": "day.get", "day": "next thursday-ish"}));

    assert_eq!(error.code, "invalid_argument");
    assert_eq!(error.exit_code, 2);
    assert!(error.message.contains("YYYY-MM-DD"), "{}", error.message);
}

// ---- adding a task ---------------------------------------------------

#[test]
fn everything_an_add_asks_for_is_one_change_and_one_undo_entry() {
    let mut world = World::new();
    world.task("First");
    world.task("Second");

    let data = world.ok(json!({
        "op": "task.add",
        "title": "Write the report",
        "place": {"kind": "day", "day": "today"},
        "focus": true,
        "due": "2026-09-10",
        "remind": "tomorrow",
        "position": 1,
    }));

    let task = &data["task"];
    assert_eq!(task["title"], "Write the report");
    assert_eq!(task["focus"], true);
    assert_eq!(task["due_on"], "2026-09-10");
    assert_eq!(task["remind_on"], "2026-09-08");
    assert_eq!(task["position"], 1);
    assert!(!data["undo"].is_null());
    assert_eq!(world.model().undo.len(), 3, "one entry for the whole add");
}

#[test]
fn one_undo_takes_back_the_whole_of_an_add() {
    let mut world = World::new();
    let data = world.ok(json!({
        "op": "task.add",
        "title": "Write the report",
        "place": {"kind": "backlog"},
        "due": "2026-09-10",
        "repeat": {"kind": "daily"},
    }));
    let id = data["task"]["id"].as_i64().expect("an id");
    assert!(!data["schedule"].is_null());

    world.ok(json!({"op": "undo.apply"}));

    let model = world.model();
    assert!(model.live_task(id).is_none(), "the task went with the undo");
    assert!(model.schedules.is_empty(), "so did the schedule it made");
    assert!(model.undo.is_empty());
}

#[test]
fn a_waiting_task_cannot_be_added_onto_a_day() {
    let mut world = World::new();
    let error = world.err(json!({
        "op": "task.add",
        "title": "Hear back",
        "place": {"kind": "day", "day": "today"},
        "waiting": true,
    }));

    assert_eq!(error.code, "invalid_argument");
    assert_eq!(error.exit_code, 2);
}

#[test]
fn focus_on_a_backlog_task_is_the_domains_own_refusal() {
    let mut world = World::new();
    let error = world.err(json!({
        "op": "task.add",
        "title": "Write the report",
        "place": {"kind": "backlog"},
        "focus": true,
    }));

    assert_eq!(error.code, "rejected");
    assert_eq!(error.exit_code, 4);
    assert_eq!(error.message, "Focus is for tasks on a day.");
    assert!(world.model().tasks.is_empty(), "the add was not written");
}

#[test]
fn a_flag_already_saying_what_it_is_asked_to_say_is_not_a_change() {
    let mut world = World::new();
    let data = world.ok(json!({
        "op": "task.add",
        "title": "Write the report",
        "place": {"kind": "backlog"},
        "focus": false,
        "waiting": false,
    }));

    assert_eq!(data["task"]["focus"], false);
    assert_eq!(world.model().undo.len(), 1, "the add alone");
}

// ---- updating a task -------------------------------------------------

#[test]
fn a_null_date_clears_it_and_an_absent_one_leaves_it() {
    let mut world = World::new();
    let id = world.backlog_task("Write the report");
    world.ok(json!({"op": "task.update", "id": id, "due": "2026-09-10", "remind": "2026-09-09"}));

    let data = world.ok(json!({"op": "task.update", "id": id, "due": null}));
    assert!(data["task"]["due_on"].is_null());
    assert_eq!(
        data["task"]["remind_on"], "2026-09-09",
        "remind was not named"
    );
}

#[test]
fn renaming_a_recurring_copy_says_which_titles_it_means() {
    let mut world = World::new();
    let id = world.task("Standup");
    world.ok(json!({"op": "schedule.create", "task": id, "rule": {"kind": "workdays"}}));

    let error = world.err(json!({"op": "task.update", "id": id, "title": "Daily standup"}));
    assert_eq!(error.code, "invalid_argument");
    assert!(error.message.contains("title_scope"), "{}", error.message);

    world.ok(json!({
        "op": "task.update", "id": id, "title": "Daily standup", "title_scope": "future",
    }));
    let schedules = world.ok(json!({"op": "schedule.list"}));
    assert_eq!(schedules["schedules"][0]["title"], "Daily standup");
}

#[test]
fn a_title_scope_of_this_leaves_the_schedule_alone() {
    let mut world = World::new();
    let id = world.task("Standup");
    world.ok(json!({"op": "schedule.create", "task": id, "rule": {"kind": "workdays"}}));
    world.ok(json!({
        "op": "task.update", "id": id, "title": "Monday standup", "title_scope": "this",
    }));

    let schedules = world.ok(json!({"op": "schedule.list"}));
    assert_eq!(schedules["schedules"][0]["title"], "Standup");
}

#[test]
fn a_title_scope_without_a_title_is_refused() {
    let mut world = World::new();
    let id = world.task("Write the report");
    let error = world.err(json!({"op": "task.update", "id": id, "title_scope": "this"}));

    assert_eq!(error.code, "invalid_argument");
}

#[test]
fn a_move_and_a_position_in_one_request_land_where_the_position_says() {
    let mut world = World::new();
    world.task("First");
    world.task("Second");
    let id = world.backlog_task("Third");

    let data = world.ok(json!({
        "op": "task.update",
        "id": id,
        "place": {"kind": "day", "day": "today"},
        "position": 1,
    }));

    assert_eq!(data["task"]["position"], 1);
    assert_eq!(
        world.model().undo.len(),
        4,
        "one entry for the whole update"
    );
}

#[test]
fn an_update_that_asks_for_nothing_is_refused() {
    let mut world = World::new();
    let id = world.task("Write the report");
    let error = world.err(json!({"op": "task.update", "id": id}));

    assert_eq!(error.code, "invalid_argument");
}

#[test]
fn a_task_that_is_gone_is_missing_rather_than_rejected() {
    let mut world = World::new();
    let error = world.err(json!({"op": "task.get", "id": 99}));

    assert_eq!(error.code, "not_found");
    assert_eq!(error.exit_code, 3);
}

// ---- several tasks at once -------------------------------------------

#[test]
fn closing_several_tasks_is_one_change_and_one_undo_entry() {
    let mut world = World::new();
    let first = world.task("First");
    let second = world.task("Second");

    let data = world.ok(json!({"op": "task.close", "ids": [first, second]}));

    assert_eq!(data["count"], 2);
    assert_eq!(ids(&data["tasks"]), vec![first, second]);
    assert_eq!(world.model().undo.len(), 3);

    world.ok(json!({"op": "undo.apply"}));
    let model = world.model();
    assert!(model.live_task(first).is_some_and(|task| task.is_open()));
    assert!(model.live_task(second).is_some_and(|task| task.is_open()));
}

#[test]
fn one_bad_id_among_many_writes_nothing() {
    let mut world = World::new();
    let first = world.task("First");
    let second = world.task("Second");

    let error = world.err(json!({"op": "task.close", "ids": [first, 99, second]}));
    assert_eq!(error.code, "not_found");

    let model = world.model();
    assert!(model.live_task(first).is_some_and(|task| task.is_open()));
    assert!(model.live_task(second).is_some_and(|task| task.is_open()));
}

#[test]
fn an_id_named_twice_is_refused() {
    let mut world = World::new();
    let id = world.task("Write the report");
    let error = world.err(json!({"op": "task.close", "ids": [id, id]}));

    assert_eq!(error.code, "invalid_argument");
    assert!(error.message.contains("twice"), "{}", error.message);
}

#[test]
fn deleting_asks_first_while_the_setting_says_to() {
    let mut world = World::new();
    let id = world.task("Write the report");
    world.ok(json!({"op": "settings.set", "settings": {"confirm_delete": true}}));

    let error = world.err(json!({"op": "task.delete", "ids": [id]}));
    assert_eq!(error.code, "confirmation_required");
    assert_eq!(error.exit_code, 2);
    assert!(world.model().live_task(id).is_some());

    world.ok(json!({"op": "task.delete", "ids": [id], "confirm": true}));
    assert!(world.model().live_task(id).is_none());
}

#[test]
fn moving_several_tasks_takes_them_all_to_the_same_place() {
    let mut world = World::new();
    let first = world.task("First");
    let second = world.task("Second");

    world.ok(json!({
        "op": "task.move", "ids": [first, second], "place": {"kind": "backlog"},
    }));

    let backlog = world.ok(json!({"op": "backlog.get"}));
    assert_eq!(ids(&backlog["ordinary"]), vec![first, second]);
}

// ---- order -----------------------------------------------------------

#[test]
fn a_position_counts_the_open_tasks_and_steps_over_the_closed_ones() {
    let mut world = World::new();
    let first = world.task("First");
    let closed = world.task("Closed");
    let third = world.task("Third");
    let fourth = world.task("Fourth");
    world.ok(json!({"op": "task.close", "ids": [closed]}));

    // The day now holds first, closed, third, fourth; the open ones are
    // first, third, fourth, so "third" is at position 2.
    let data = world.ok(json!({"op": "task.get", "id": third}));
    assert_eq!(data["task"]["position"], 2);

    let data = world.ok(json!({"op": "task.get", "id": closed}));
    assert!(data["task"]["position"].is_null(), "a closed task has none");

    let data = world.ok(json!({"op": "task.reorder", "id": fourth, "position": 1}));
    assert_eq!(ids(&data["order"]), vec![fourth, first, third]);
}

#[test]
fn reordering_leaves_every_closed_task_in_the_slot_it_had() {
    let mut world = World::new();
    let first = world.task("First");
    let closed = world.task("Closed");
    let third = world.task("Third");
    world.ok(json!({"op": "task.close", "ids": [closed]}));

    let before = world
        .model()
        .task(closed)
        .map(|task| task.position)
        .expect("the closed task");

    world.ok(json!({
        "op": "day.reorder",
        "place": {"kind": "day", "day": "today"},
        "ids": [third, first],
    }));

    let model = world.model();
    let after = model.task(closed).map(|task| task.position);
    assert_eq!(after, Some(before), "the closed task did not move");

    let order: Vec<Id> = model
        .place(Place::Day(
            model.settings.working_day(&at("2026-09-07T09:00:00")),
        ))
        .iter()
        .map(|task| task.id)
        .collect();
    assert_eq!(order, vec![third, closed, first], "the open ones swapped");
}

#[test]
fn before_and_after_put_a_task_beside_another() {
    let mut world = World::new();
    let first = world.task("First");
    let second = world.task("Second");
    let third = world.task("Third");

    let data = world.ok(json!({"op": "task.reorder", "id": third, "before": first}));
    assert_eq!(ids(&data["order"]), vec![third, first, second]);

    let data = world.ok(json!({"op": "task.reorder", "id": third, "after": second}));
    assert_eq!(ids(&data["order"]), vec![first, second, third]);
}

#[test]
fn a_position_past_the_end_says_where_the_end_is() {
    let mut world = World::new();
    let id = world.task("First");
    world.task("Second");

    let error = world.err(json!({"op": "task.reorder", "id": id, "position": 4}));
    assert_eq!(error.code, "invalid_argument");
    assert!(
        error.message.contains("last position is 2"),
        "{}",
        error.message
    );
}

#[test]
fn a_reorder_says_where_a_task_goes_exactly_once() {
    let mut world = World::new();
    let first = world.task("First");
    let second = world.task("Second");

    assert_eq!(
        world
            .err(json!({"op": "task.reorder", "id": first, "position": 1, "after": second}))
            .code,
        "invalid_argument"
    );
    assert_eq!(
        world.err(json!({"op": "task.reorder", "id": first})).code,
        "invalid_argument"
    );
}

#[test]
fn a_full_order_is_of_every_open_task_of_the_place_and_no_other() {
    let mut world = World::new();
    let first = world.task("First");
    let second = world.task("Second");
    let closed = world.task("Closed");
    world.ok(json!({"op": "task.close", "ids": [closed]}));
    let elsewhere = world.backlog_task("Elsewhere");

    let day = json!({"kind": "day", "day": "today"});
    let refused = |world: &mut World, ids: Value| -> Error {
        world.err(json!({"op": "day.reorder", "place": day, "ids": ids}))
    };

    assert_eq!(refused(&mut world, json!([first])).code, "invalid_argument");
    assert_eq!(
        refused(&mut world, json!([first, second, first])).code,
        "invalid_argument"
    );
    assert_eq!(
        refused(&mut world, json!([first, second, closed])).code,
        "invalid_argument"
    );
    assert_eq!(
        refused(&mut world, json!([first, second, elsewhere])).code,
        "invalid_argument"
    );
    assert_eq!(
        refused(&mut world, json!([first, second, 99])).code,
        "not_found"
    );

    let data = world.ok(json!({"op": "day.reorder", "place": day, "ids": [second, first]}));
    assert_eq!(ids(&data["order"]), vec![second, first]);
}

#[test]
fn an_order_that_is_already_the_order_writes_nothing() {
    let mut world = World::new();
    let first = world.task("First");
    let second = world.task("Second");
    let entries = world.model().undo.len();

    let data = world.ok(json!({
        "op": "day.reorder",
        "place": {"kind": "day", "day": "today"},
        "ids": [first, second],
    }));

    assert!(data["undo"].is_null());
    assert_eq!(world.model().undo.len(), entries);
}

#[test]
fn a_whole_order_of_one_place_is_one_undo_entry() {
    let mut world = World::new();
    let first = world.task("First");
    let second = world.task("Second");
    let third = world.task("Third");
    let entries = world.model().undo.len();

    world.ok(json!({
        "op": "day.reorder",
        "place": {"kind": "day", "day": "today"},
        "ids": [third, second, first],
    }));
    assert_eq!(world.model().undo.len(), entries + 1);

    world.ok(json!({"op": "undo.apply"}));
    let data = world.ok(json!({"op": "day.get"}));
    assert_eq!(ids(&data["plan"]), vec![first, second, third]);
}

// ---- the views -------------------------------------------------------

#[test]
fn a_day_is_drawn_in_the_groups_the_pane_draws() {
    let mut world = World::new();
    let focus = world.task("Focus");
    let plan = world.task("Plan");
    let done = world.task("Done");
    let gone = world.task("Gone");
    world.ok(json!({"op": "task.update", "id": focus, "focus": true}));
    world.ok(json!({"op": "task.close", "ids": [done]}));
    world.ok(json!({"op": "task.move", "ids": [gone], "place": {"kind": "backlog"}}));

    let data = world.ok(json!({"op": "day.get"}));
    assert_eq!(ids(&data["focus"]), vec![focus]);
    assert_eq!(ids(&data["plan"]), vec![plan]);
    assert_eq!(ids(&data["done"]), vec![done]);
    assert_eq!(ids(&data["moved"]), vec![gone]);
    assert_eq!(data["counts"]["planned"], 4);
    assert_eq!(data["counts"]["open"], 2);
}

#[test]
fn the_backlog_shows_the_waiting_tasks_apart_and_the_schedules_under_them() {
    let mut world = World::new();
    let ordinary = world.backlog_task("Ordinary");
    let waiting = world.backlog_task("Waiting");
    world.ok(json!({"op": "task.update", "id": waiting, "waiting": true}));
    world.ok(json!({"op": "schedule.create", "task": ordinary, "rule": {"kind": "daily"}}));

    let data = world.ok(json!({"op": "backlog.get"}));
    assert_eq!(ids(&data["ordinary"]), vec![ordinary]);
    assert_eq!(ids(&data["waiting"]), vec![waiting]);
    assert_eq!(data["waiting_count"], 1);
    assert_eq!(data["schedules"][0]["rule"]["kind"], "daily");
}

#[test]
fn a_day_nothing_was_planned_for_is_an_empty_view_rather_than_an_error() {
    let mut world = World::new();
    let data = world.ok(json!({"op": "day.get", "day": "2001-01-01"}));

    assert_eq!(data["day"], "2001-01-01");
    assert_eq!(data["counts"]["planned"], 0);
}

#[test]
fn search_matches_a_title_whatever_case_it_was_written_in() {
    let mut world = World::new();
    let open = world.task("Write the REPORT");
    let closed = world.task("Report back");
    world.ok(json!({"op": "task.close", "ids": [closed]}));
    world.task("Something else");

    let data = world.ok(json!({"op": "search", "text": "report"}));
    assert_eq!(data["total"], 2);
    assert_eq!(ids(&data["open"]), vec![open]);
    assert_eq!(ids(&data["closed"]), vec![closed]);
}

#[test]
fn history_lists_the_days_that_have_a_placement_newest_first() {
    let mut world = World::new();
    world.task("Today");
    world.clock("2026-09-09T09:00:00");
    world.task("Wednesday");

    let data = world.ok(json!({"op": "history.list"}));
    assert_eq!(data["total_days"], 2);
    assert_eq!(data["stretches"][0]["days"][0]["day"], "2026-09-09");

    let data = world.ok(json!({"op": "history.list", "limit": 1}));
    assert_eq!(data["total_days"], 1);
    assert_eq!(
        world.err(json!({"op": "history.list", "limit": 0})).code,
        "invalid_argument"
    );
}

#[test]
fn a_task_carries_its_place_its_dates_and_its_schedule() {
    let mut world = World::new();
    let id = world.task("Standup");
    world.ok(json!({"op": "schedule.create", "task": id, "rule": {"kind": "workdays"}}));
    world.ok(json!({"op": "task.update", "id": id, "due": "2026-09-10"}));

    let data = world.ok(json!({"op": "task.get", "id": id}));
    let task = &data["task"];
    assert_eq!(task["place"], json!({"kind": "day", "day": "2026-09-07"}));
    assert_eq!(task["open"], true);
    assert_eq!(task["due_on"], "2026-09-10");
    assert_eq!(task["schedule"]["scheduled_on"], "2026-09-07");
    assert_eq!(task["schedule"]["rule"], json!({"kind": "workdays"}));
}

#[test]
fn listing_tasks_filters_by_place_and_by_state() {
    let mut world = World::new();
    let open = world.task("Open");
    let closed = world.task("Closed");
    world.ok(json!({"op": "task.close", "ids": [closed]}));
    let backlog = world.backlog_task("Backlog");

    let all = world.ok(json!({"op": "task.list"}));
    assert_eq!(
        ids(&all["tasks"]),
        vec![open, closed, backlog],
        "days before the backlog"
    );

    let only_open = world.ok(json!({"op": "task.list", "state": "open"}));
    assert_eq!(ids(&only_open["tasks"]), vec![open, backlog]);

    let only_backlog = world.ok(json!({"op": "task.list", "place": {"kind": "backlog"}}));
    assert_eq!(ids(&only_backlog["tasks"]), vec![backlog]);
}

// ---- schedules -------------------------------------------------------

#[test]
fn a_preview_shows_the_dates_a_rule_that_is_not_saved_yet_falls_on() {
    let mut world = World::new();
    let data = world.ok(json!({
        "op": "schedule.preview",
        "rule": {"kind": "weekly", "weekdays": ["wed", "fri"]},
        "count": 4,
    }));

    assert_eq!(
        data["dates"],
        json!(["2026-09-09", "2026-09-11", "2026-09-16", "2026-09-18"])
    );
}

#[test]
fn a_preview_of_a_saved_schedule_starts_where_generation_left_off() {
    let mut world = World::new();
    let id = world.task("Standup");
    let data = world.ok(json!({"op": "schedule.create", "task": id, "rule": {"kind": "workdays"}}));
    let schedule = data["schedule"]["id"].as_i64().expect("an id");

    let data = world.ok(json!({"op": "schedule.preview", "schedule": schedule}));
    assert_eq!(data["after"], "2026-09-07");
    assert_eq!(
        data["dates"],
        json!(["2026-09-08", "2026-09-09", "2026-09-10"])
    );
}

#[test]
fn a_rule_the_public_schema_does_not_allow_is_refused() {
    let mut world = World::new();
    let refused = |world: &mut World, rule: Value| -> Error {
        world.err(json!({"op": "schedule.preview", "rule": rule}))
    };

    assert_eq!(
        refused(&mut world, json!({"kind": "yearly"})).code,
        "invalid_argument"
    );
    assert_eq!(
        refused(&mut world, json!({"kind": "weekly", "weekdays": []})).code,
        "invalid_argument"
    );
    assert_eq!(
        refused(
            &mut world,
            json!({"kind": "weekly", "weekdays": ["mon", "mon"]})
        )
        .code,
        "invalid_argument"
    );
    assert_eq!(
        refused(&mut world, json!({"kind": "monthly", "day": 32})).code,
        "invalid_argument"
    );
    assert_eq!(
        refused(&mut world, json!({"kind": "daily", "weekdays": ["mon"]})).code,
        "invalid_argument"
    );
    assert_eq!(
        refused(
            &mut world,
            json!({"kind": "every_n_weeks", "n": 0, "from": "today"})
        )
        .code,
        "invalid_argument"
    );
}

#[test]
fn a_schedule_takes_a_new_title_a_new_rule_or_both_at_once() {
    let mut world = World::new();
    let task = world.task("Standup");
    let data = world.ok(json!({"op": "schedule.create", "task": task, "rule": {"kind": "daily"}}));
    let id = data["schedule"]["id"].as_i64().expect("an id");
    let entries = world.model().undo.len();

    let data = world.ok(json!({
        "op": "schedule.update",
        "id": id,
        "title": "Team standup",
        "rule": {"kind": "weekly", "weekdays": ["mon"]},
    }));

    assert_eq!(data["schedule"]["title"], "Team standup");
    assert_eq!(data["schedule"]["rule"]["kind"], "weekly");
    assert_eq!(world.model().undo.len(), entries + 1, "both in one entry");

    assert_eq!(
        world.err(json!({"op": "schedule.update", "id": id})).code,
        "invalid_argument"
    );
    assert_eq!(
        world.err(json!({"op": "schedule.get", "id": 99})).code,
        "not_found"
    );
}

#[test]
fn stopping_a_schedule_keeps_its_copies_and_takes_it_off_the_list() {
    let mut world = World::new();
    let task = world.task("Standup");
    let data = world.ok(json!({"op": "schedule.create", "task": task, "rule": {"kind": "daily"}}));
    let id = data["schedule"]["id"].as_i64().expect("an id");

    let data = world.ok(json!({"op": "schedule.stop", "id": id}));
    assert_eq!(data["schedule"]["stopped"], true);
    assert_eq!(data["schedule"]["copies"], 1);

    assert_eq!(world.ok(json!({"op": "schedule.list"}))["total"], 0);
    assert_eq!(
        world.ok(json!({"op": "schedule.list", "include_stopped": true}))["total"],
        1
    );
}

// ---- notes -----------------------------------------------------------

#[test]
fn a_note_keeps_its_newlines_and_its_unicode() {
    let mut world = World::new();
    let body = "Første linje\n\ntabbed:\tvalue 🌍\n";
    let data = world.ok(json!({"op": "note.create", "body": body}));
    let id = data["note"]["id"].as_i64().expect("an id");

    assert_eq!(data["note"]["body"], body);
    let data = world.ok(json!({"op": "note.get", "id": id}));
    assert_eq!(data["note"]["body"], body);
}

#[test]
fn a_note_and_its_body_arrive_as_one_change() {
    let mut world = World::new();
    world.ok(json!({"op": "note.create", "body": "Remember to mention X"}));
    assert_eq!(world.model().undo.len(), 1);

    world.ok(json!({"op": "undo.apply"}));
    assert!(world.model().notes.values().all(|note| !note.is_live()));
}

#[test]
fn replacing_a_note_can_be_taken_back() {
    let mut world = World::new();
    let data = world.ok(json!({"op": "note.create", "body": "First"}));
    let id = data["note"]["id"].as_i64().expect("an id");

    let data = world.ok(json!({"op": "note.update", "id": id, "body": "Second"}));
    assert_eq!(data["note"]["body"], "Second");
    assert!(!data["undo"].is_null());

    world.ok(json!({"op": "undo.apply"}));
    let data = world.ok(json!({"op": "note.get", "id": id}));
    assert_eq!(data["note"]["body"], "First");
}

#[test]
fn an_archived_note_leaves_the_list_and_is_listed_on_its_own() {
    let mut world = World::new();
    let older = world.ok(json!({"op": "note.create", "body": "Older"}))["note"]["id"].clone();
    world.clock("2026-09-07T10:00:00");
    let newer = world.ok(json!({"op": "note.create", "body": "Newer"}))["note"]["id"].clone();
    assert!(world.ok(json!({"op": "note.get", "id": older}))["note"]["archived_at"].is_null());

    world.clock("2026-09-07T11:00:00");
    let data = world.ok(json!({"op": "note.archive", "id": older}));
    assert_eq!(
        data["note"]["archived_at"],
        "2026-09-07T11:00:00+02:00[Europe/Copenhagen]"
    );
    assert_eq!(data["undo"]["label"], "Archived \"Older\"");
    world.clock("2026-09-07T12:00:00");
    world.ok(json!({"op": "note.archive", "id": newer}));

    let list = world.ok(json!({"op": "note.list"}));
    assert_eq!(list["count"], 0);
    assert_eq!(list["notes"], json!([]));

    let archive = world.ok(json!({"op": "note.list", "archived": true}));
    assert_eq!(archive["count"], 2);
    assert_eq!(archive["notes"][0]["first_line"], "Newer");
    assert_eq!(archive["notes"][1]["first_line"], "Older");
    assert_eq!(
        archive["notes"][1]["archived_at"],
        "2026-09-07T11:00:00+02:00[Europe/Copenhagen]"
    );

    let data = world.ok(json!({"op": "note.unarchive", "id": older}));
    assert!(data["note"]["archived_at"].is_null());
    assert_eq!(
        world.ok(json!({"op": "note.list"}))["notes"][0]["first_line"],
        "Older"
    );

    world.ok(json!({"op": "undo.apply"}));
    let archive = world.ok(json!({"op": "note.list", "archived": true}));
    assert_eq!(
        archive["notes"][1]["first_line"], "Older",
        "back in its place"
    );
}

#[test]
fn an_archived_note_is_read_changed_checked_and_deleted_like_any_other() {
    let mut world = World::new();
    let id = world.ok(json!({"op": "note.create", "body": "Milk"}))["note"]["id"].clone();
    world.ok(json!({"op": "note.archive", "id": id}));

    assert_eq!(
        world.ok(json!({"op": "note.get", "id": id}))["note"]["body"],
        "Milk"
    );
    let data = world.ok(json!({"op": "note.update", "id": id, "body": "Mlik"}));
    assert_eq!(data["note"]["body"], "Mlik");
    assert!(!data["note"]["archived_at"].is_null());
    let data = world.ok(json!({"op": "note.check", "note": id}));
    assert_eq!(data["misspellings"][0]["word"], "Mlik");
    world.ok(json!({"op": "note.delete", "id": id, "confirm": true}));
    assert_eq!(
        world.ok(json!({"op": "note.list", "archived": true}))["count"],
        0
    );
}

#[test]
fn a_note_is_taken_out_of_spell_checking_and_still_checked_when_asked() {
    let mut world = World::new();
    let id = world.ok(json!({"op": "note.create", "body": "Xqzt"}))["note"]["id"].clone();
    assert_eq!(
        world.ok(json!({"op": "note.get", "id": id}))["note"]["spell_check"],
        true
    );

    let data = world.ok(json!({"op": "note.spell_check", "id": id, "check": false}));
    assert_eq!(data["note"]["spell_check"], false);
    assert_eq!(data["undo"]["label"], "Spell check off for \"Xqzt\"");

    let refused = world.err(json!({"op": "note.spell_check", "id": id, "check": false}));
    assert_eq!(refused.code, "rejected");
    assert_eq!(refused.message, "Spell check is already off for that note.");

    let data = world.ok(json!({"op": "note.check", "note": id}));
    assert_eq!(data["misspellings"][0]["word"], "Xqzt");

    world.ok(json!({"op": "undo.apply"}));
    assert_eq!(
        world.ok(json!({"op": "note.get", "id": id}))["note"]["spell_check"],
        true
    );
    assert_eq!(
        world
            .err(json!({"op": "note.spell_check", "id": 99, "check": false}))
            .code,
        "not_found"
    );
}

#[test]
fn a_note_is_archived_from_the_list_and_unarchived_from_the_archive() {
    let mut world = World::new();
    let id = world.ok(json!({"op": "note.create", "body": "Milk"}))["note"]["id"].clone();

    let refused = world.err(json!({"op": "note.unarchive", "id": id}));
    assert_eq!(refused.code, "rejected");
    assert_eq!(refused.message, "That note is not archived.");

    world.ok(json!({"op": "note.archive", "id": id}));
    let refused = world.err(json!({"op": "note.archive", "id": id}));
    assert_eq!(refused.code, "rejected");
    assert_eq!(refused.message, "That note is already archived.");

    assert_eq!(
        world.err(json!({"op": "note.archive", "id": 99})).code,
        "not_found"
    );
}

#[test]
fn the_notes_list_is_newest_first_and_shows_the_first_line() {
    let mut world = World::new();
    world.ok(json!({"op": "note.create", "body": "Older\nsecond line"}));
    world.clock("2026-09-07T10:00:00");
    world.ok(json!({"op": "note.create", "body": "Newer"}));

    let data = world.ok(json!({"op": "note.list"}));
    assert_eq!(data["count"], 2);
    assert_eq!(data["notes"][0]["first_line"], "Newer");
    assert_eq!(data["notes"][1]["first_line"], "Older");
    assert!(data["notes"][0]["body"].is_null());

    let data = world.ok(json!({"op": "note.list", "include_body": true}));
    assert_eq!(data["notes"][1]["body"], "Older\nsecond line");
}

#[test]
fn a_check_reports_the_word_behind_each_range() {
    let mut world = World::new();
    let data = world.ok(json!({"op": "note.check", "text": "The reprot is redy"}));

    let words: Vec<&str> = data["misspellings"]
        .as_array()
        .expect("an array")
        .iter()
        .map(|found| found["word"].as_str().expect("a word"))
        .collect();
    assert_eq!(words, vec!["reprot", "redy"]);
    assert_eq!(data["misspellings"][0]["start"], 4);
    assert_eq!(data["misspellings"][0]["end"], 10);
    assert_eq!(data["spell_check_notes"], false);
}

#[test]
fn a_check_counts_in_grapheme_clusters() {
    let mut world = World::new();
    // The family emoji is one cluster and several chars.
    let data = world.ok(json!({"op": "note.check", "text": "👨‍👩‍👧 reprot"}));

    assert_eq!(data["misspellings"][0]["start"], 2);
    assert_eq!(data["misspellings"][0]["word"], "reprot");
}

#[test]
fn a_word_in_the_personal_dictionary_is_not_a_misspelling() {
    let mut world = World::new();
    let text = "Zylquist shipped it";
    let data = world.ok(json!({"op": "note.check", "text": text}));
    assert_eq!(data["misspellings"].as_array().map(Vec::len), Some(1));

    world.ok(json!({"op": "dictionary.add", "word": "Zylquist"}));
    let data = world.ok(json!({"op": "note.check", "text": text}));
    assert_eq!(data["misspellings"].as_array().map(Vec::len), Some(0));
}

#[test]
fn a_check_is_of_one_note_or_one_piece_of_text() {
    let mut world = World::new();
    assert_eq!(
        world.err(json!({"op": "note.check"})).code,
        "invalid_argument"
    );
    assert_eq!(
        world
            .err(json!({"op": "note.check", "note": 1, "text": "x"}))
            .code,
        "invalid_argument"
    );
}

// ---- the settings ----------------------------------------------------

#[test]
fn the_settings_come_back_as_values_rather_than_as_text() {
    let mut world = World::new();
    let data = world.ok(json!({"op": "settings.get"}));

    assert_eq!(data["settings"]["day_starts_at"], 5);
    assert_eq!(
        data["settings"]["work_days"],
        json!(["mon", "tue", "wed", "thu", "fri"])
    );
    assert_eq!(
        data["settings"]["window_size"],
        json!({"width": 870, "height": 650})
    );
    assert_eq!(data["settings"]["date_style"], "locale");
    assert_eq!(data["date_order"], "day_first");
}

#[test]
fn the_date_order_follows_the_setting_and_falls_back_to_the_locale() {
    let mut world = World::new();
    let data = world.ok(json!({"op": "settings.set", "settings": {"date_style": "month_first"}}));
    assert_eq!(data["date_order"], "month_first");

    let response = world.envelope(json!({"op": "settings.get"}));
    assert_eq!(response["context"]["date_order"], "month_first");
}

#[test]
fn a_setting_outside_its_range_is_refused_rather_than_held_to_it() {
    let mut world = World::new();
    let refused = |world: &mut World, settings: Value| -> Error {
        world.err(json!({"op": "settings.set", "settings": settings}))
    };

    assert_eq!(
        refused(&mut world, json!({"day_starts_at": 48})).code,
        "invalid_argument"
    );
    assert_eq!(
        refused(&mut world, json!({"due_ahead_days": -1})).code,
        "invalid_argument"
    );
    assert_eq!(
        refused(&mut world, json!({"message_seconds": 61})).code,
        "invalid_argument"
    );
    assert_eq!(
        refused(
            &mut world,
            json!({"window_size": {"width": 10, "height": 650}})
        )
        .code,
        "invalid_argument"
    );
    assert_eq!(
        refused(&mut world, json!({"mouse": "yes"})).code,
        "invalid_argument"
    );
    assert_eq!(
        refused(&mut world, json!({"week_starts_on": "friday"})).code,
        "invalid_argument"
    );
    assert_eq!(
        refused(&mut world, json!({"work_days": []})).code,
        "invalid_argument"
    );
    assert_eq!(
        refused(&mut world, json!({"work_days": ["mon", "mon"]})).code,
        "invalid_argument"
    );
    assert_eq!(
        refused(&mut world, json!({"colour": "blue"})).code,
        "invalid_argument"
    );
    assert_eq!(refused(&mut world, json!({})).code, "invalid_argument");

    assert_eq!(
        world.ok(json!({"op": "settings.get"}))["settings"]["day_starts_at"],
        5
    );
}

#[test]
fn a_window_setting_is_saved_and_reported_rather_than_handed_to_anybody() {
    let mut world = World::new();
    let data = world.ok(json!({
        "op": "settings.set",
        "settings": {"window_size": {"width": 1010, "height": 755}},
    }));

    assert_eq!(data["desktop"]["window_rule_changed"], true);
    assert_eq!(
        data["desktop"]["window_size"],
        json!({"width": 1010, "height": 755})
    );
    assert!(data["undo"].is_null());

    let data = world.ok(json!({"op": "settings.set", "settings": {"mouse": false}}));
    assert_eq!(data["desktop"]["window_rule_changed"], false);
}

#[test]
fn moving_the_hour_a_day_starts_at_moves_the_day_the_response_reports() {
    let mut world = World::new();
    world.clock("2026-09-07T04:00:00");
    let response = world.envelope(json!({"op": "settings.set", "settings": {"day_starts_at": 3}}));

    assert_eq!(response["context"]["today"], "2026-09-07");
}

// ---- the dictionary --------------------------------------------------

#[test]
fn a_dictionary_entry_is_found_by_the_word_in_any_capitalisation() {
    let mut world = World::new();
    let data = world.ok(json!({"op": "dictionary.add", "word": "Zylquist"}));
    assert_eq!(data["word"], json!({"key": "zylquist", "word": "Zylquist"}));
    assert!(data["undo"].is_null());

    world.ok(json!({"op": "dictionary.update", "key": "ZYLQUIST", "word": "ZylQuist"}));
    let data = world.ok(json!({"op": "dictionary.list"}));
    assert_eq!(
        data["words"],
        json!([{"key": "zylquist", "word": "ZylQuist"}])
    );

    world.ok(json!({"op": "dictionary.delete", "key": "zylquist"}));
    assert_eq!(world.ok(json!({"op": "dictionary.list"}))["count"], 0);
}

#[test]
fn a_word_that_is_not_in_the_dictionary_is_missing() {
    let mut world = World::new();
    assert_eq!(
        world
            .err(json!({"op": "dictionary.delete", "key": "nope"}))
            .code,
        "not_found"
    );
    assert_eq!(
        world
            .err(json!({"op": "dictionary.update", "key": "nope", "word": "Nope"}))
            .code,
        "not_found"
    );
}

#[test]
fn a_dictionary_entry_is_one_word() {
    let mut world = World::new();
    let error = world.err(json!({"op": "dictionary.add", "word": "two words"}));

    assert_eq!(error.code, "rejected");
    assert_eq!(error.exit_code, 4);
}

// ---- generation, the review gate and undo -----------------------------

#[test]
fn reading_makes_no_copies_and_does_not_spend_the_review() {
    let mut world = World::new();
    let id = world.task("Standup");
    world.ok(json!({"op": "schedule.create", "task": id, "rule": {"kind": "daily"}}));
    world.clock("2026-09-09T09:00:00");

    for op in [
        "day.get",
        "backlog.get",
        "review.get",
        "task.list",
        "history.list",
    ] {
        let response = world.envelope(json!({"op": op}));
        assert_eq!(response["context"]["recurrence_pending"], true, "{op}");
    }

    let model = world.model();
    assert_eq!(model.tasks.len(), 1, "no copy was made by looking");
    assert!(
        !model.meta.contains_key("review_on"),
        "the gate was not written"
    );
}

#[test]
fn refresh_makes_the_copies_and_puts_nothing_on_the_undo_stack() {
    let mut world = World::new();
    let id = world.task("Standup");
    world.ok(json!({"op": "schedule.create", "task": id, "rule": {"kind": "daily"}}));
    let entries = world.model().undo.len();
    world.clock("2026-09-09T09:00:00");

    let response = world.envelope(json!({"op": "refresh"}));
    assert_eq!(response["data"]["count"], 2, "the 8th and the 9th");
    assert!(response["data"]["undo"].is_null());
    assert_eq!(response["context"]["recurrence_pending"], false);
    assert_eq!(world.model().undo.len(), entries);

    let response = world.envelope(json!({"op": "refresh"}));
    assert_eq!(
        response["data"]["count"], 0,
        "a second refresh owes nothing"
    );
}

#[test]
fn the_review_gate_moves_once_a_day_and_only_when_asked() {
    let mut world = World::new();
    world.task("Left open");
    world.clock("2026-09-09T09:00:00");

    let data = world.ok(json!({"op": "review.get"}));
    assert_eq!(data["pile"]["total"], 1);
    assert_eq!(data["gate"]["would_open"], true);
    assert_eq!(data["gate"]["started_today"], false);
    assert!(data["gate"]["review_on"].is_null());

    let data = world.ok(json!({"op": "review.start"}));
    assert_eq!(data["started"], true);
    assert_eq!(data["gate"]["review_on"], "2026-09-09");
    assert_eq!(data["gate"]["would_open"], false);
    assert!(data["undo"].is_null());

    let data = world.ok(json!({"op": "review.start"}));
    assert_eq!(data["started"], false, "the same day changes nothing");
}

#[test]
fn a_due_backlog_task_surfaces_and_a_waiting_one_does_not() {
    let mut world = World::new();
    let due = world.backlog_task("Due");
    let waiting = world.backlog_task("Waiting");
    world.ok(json!({"op": "task.update", "id": due, "due": "2026-09-07"}));
    world.ok(json!({
        "op": "task.update", "id": waiting, "due": "2026-09-07", "waiting": true,
    }));

    let data = world.ok(json!({"op": "review.get"}));
    assert_eq!(ids(&data["surfaced"]["due"]), vec![due]);
}

#[test]
fn undo_reports_the_entry_it_would_take_back() {
    let mut world = World::new();
    world.task("Write the report");

    let data = world.ok(json!({"op": "undo.get"}));
    assert_eq!(data["depth"], 1);
    let id = data["entry"]["id"].as_i64().expect("an id");
    assert!(
        data["entry"]["label"]
            .as_str()
            .is_some_and(|label| label.starts_with("Added")),
        "{data}"
    );

    let data = world.ok(json!({"op": "undo.apply", "expected_id": id}));
    assert_eq!(data["applied"], true);
    assert_eq!(data["entry_id"], id);
    assert_eq!(data["depth"], 0);
}

#[test]
fn an_undo_of_an_entry_that_is_no_longer_on_top_is_a_conflict() {
    let mut world = World::new();
    world.task("First");
    let id = world.ok(json!({"op": "undo.get"}))["entry"]["id"]
        .as_i64()
        .expect("an id");
    world.task("Second");

    let error = world.err(json!({"op": "undo.apply", "expected_id": id}));
    assert_eq!(error.code, "conflict");
    assert_eq!(error.exit_code, 5);
    assert_eq!(world.model().undo.len(), 2, "nothing was taken back");
}

#[test]
fn there_is_nothing_to_undo_on_an_empty_stack() {
    let mut world = World::new();
    let data = world.ok(json!({"op": "undo.get"}));
    assert!(data["entry"].is_null());

    let error = world.err(json!({"op": "undo.apply"}));
    assert_eq!(error.code, "not_found");
    assert_eq!(error.exit_code, 3);
}

#[test]
fn a_guarded_undo_rejects_an_identity_popped_before_a_new_action() {
    let mut world = World::new();
    world.task("First");
    let id = world.ok(json!({"op": "undo.get"}))["entry"]["id"]
        .as_i64()
        .unwrap();
    world.ok(json!({"op": "undo.apply", "expected_id": id}));
    world.task("Second");
    let before = world.model();
    let error = world.err(json!({"op": "undo.apply", "expected_id": id}));
    assert_eq!(error.exit_code, 5);
    assert_eq!(world.model(), before);
}
