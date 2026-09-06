-- The settings table, from DOMAIN.md section 19. One row per key; a key
-- the binary does not know is left alone, and a missing one is the
-- default.

CREATE TABLE settings (
    key    TEXT PRIMARY KEY,
    value  TEXT NOT NULL
);

PRAGMA user_version = 2;
