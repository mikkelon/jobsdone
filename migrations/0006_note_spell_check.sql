-- Whether a note is spell-checked while the setting is on: 1 for every
-- note until it is taken out of checking on its own, 0 after.
ALTER TABLE notes ADD COLUMN spell_check INTEGER NOT NULL DEFAULT 1;

PRAGMA user_version = 6;
