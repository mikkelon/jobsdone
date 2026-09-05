-- The first migration, copied from DOMAIN.md section 17. That document
-- is where the schema is designed; this file is what runs.

CREATE TABLE tasks (
    id            INTEGER PRIMARY KEY,
    title         TEXT    NOT NULL,
    day           TEXT,
    position      INTEGER NOT NULL,
    focus         INTEGER NOT NULL DEFAULT 0 CHECK (focus IN (0, 1)),
    waiting       INTEGER NOT NULL DEFAULT 0 CHECK (waiting IN (0, 1)),
    closed_at     TEXT,
    due_on        TEXT,
    remind_on     TEXT,
    schedule_id   INTEGER REFERENCES schedules (id),
    scheduled_on  TEXT,
    created_at    TEXT    NOT NULL,
    deleted_at    TEXT,
    CHECK (title <> '' AND title = trim(title) AND instr(title, char(10)) = 0),
    CHECK (waiting = 0 OR day IS NULL),
    CHECK ((schedule_id IS NULL) = (scheduled_on IS NULL))
);

CREATE INDEX tasks_place ON tasks (day, position) WHERE deleted_at IS NULL;
CREATE UNIQUE INDEX tasks_copy ON tasks (schedule_id, scheduled_on)
    WHERE schedule_id IS NOT NULL;

CREATE TABLE placements (
    task_id     INTEGER NOT NULL REFERENCES tasks (id),
    day         TEXT    NOT NULL,
    placed_at   TEXT    NOT NULL,
    from_place  TEXT    NOT NULL,
    PRIMARY KEY (task_id, day)
);

CREATE INDEX placements_day ON placements (day);

CREATE TABLE schedules (
    id                 INTEGER PRIMARY KEY,
    title              TEXT NOT NULL,
    rule               TEXT NOT NULL,
    generated_through  TEXT NOT NULL,
    stopped_on         TEXT,
    created_at         TEXT NOT NULL
);

CREATE TABLE notes (
    id          INTEGER PRIMARY KEY,
    body        TEXT NOT NULL DEFAULT '',
    created_at  TEXT NOT NULL,
    updated_at  TEXT NOT NULL,
    deleted_at  TEXT
);

CREATE TABLE meta (
    key    TEXT PRIMARY KEY,
    value  TEXT NOT NULL
);

CREATE TABLE undo_log (
    id       INTEGER PRIMARY KEY,
    at       TEXT NOT NULL,
    label    TEXT NOT NULL,
    inverse  TEXT NOT NULL
);

PRAGMA user_version = 1;
