-- The settings table, from DOMAIN.md section 19. One row per key; a
-- missing key is that setting's default, and a key the binary does not
-- know is ignored.

CREATE TABLE settings (
    key    TEXT PRIMARY KEY,
    value  TEXT NOT NULL
);

PRAGMA user_version = 2;
