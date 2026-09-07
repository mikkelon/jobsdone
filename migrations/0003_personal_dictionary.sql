-- The personal dictionary table, from DOMAIN.md section 17. One row per
-- word: the canonical key a checker asks with, and the word as it was
-- typed, which is what the manager shows.

CREATE TABLE personal_dictionary (
    key   TEXT PRIMARY KEY,
    word  TEXT NOT NULL,
    CHECK (key <> '' AND word <> '' AND word = trim(word))
);

PRAGMA user_version = 3;
