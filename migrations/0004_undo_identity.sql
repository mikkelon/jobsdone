-- Undo identities must outlive entries removed by undo or stack truncation.
-- Older schemas cannot recover identities already recycled by their allocator.
INSERT INTO meta (key, value)
    SELECT 'undo_high_water', CAST(COALESCE(MAX(id), 0) AS TEXT) FROM undo_log;

-- Already-running older clients do not repeat the schema-version check.
-- Enforce the identity contract even for their old max(stack)+1 allocator.
CREATE TRIGGER undo_identity_guard BEFORE INSERT ON undo_log
BEGIN
    SELECT CASE WHEN NOT EXISTS (
        SELECT 1 FROM meta
        WHERE key = 'undo_high_water'
          AND value = CAST(CAST(value AS INTEGER) AS TEXT)
          AND CAST(value AS INTEGER) >= 0
          AND NEW.id > CAST(value AS INTEGER)
    ) THEN RAISE(ABORT, 'invalid or reused undo identity') END;
END;

CREATE TRIGGER undo_identity_advance AFTER INSERT ON undo_log
BEGIN
    UPDATE meta SET value = CAST(NEW.id AS TEXT) WHERE key = 'undo_high_water';
END;

PRAGMA user_version = 4;
