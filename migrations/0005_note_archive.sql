-- A note put out of the way without being thrown away. Archived is a
-- state of a live note, separate from deleted.
ALTER TABLE notes ADD COLUMN archived_at TEXT;

PRAGMA user_version = 5;
