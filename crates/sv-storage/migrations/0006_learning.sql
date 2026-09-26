-- FilePath: crates/sv-storage/migrations/0006_learning.sql
-- Learning from corrections adds the words a user fixes after a paste to the dictionary. Learned
-- rows are marked so the UI can tell them apart; every row written before this is the user's own.
-- A learned word the user deletes is remembered in dictionary_rejected so it is never learned
-- again. history.edited_text keeps what the pasted text read after the user corrected it.

ALTER TABLE dictionary ADD COLUMN source TEXT NOT NULL DEFAULT 'manual';  -- 'manual' | 'learned'

CREATE TABLE dictionary_rejected (
    phrase      TEXT PRIMARY KEY NOT NULL COLLATE NOCASE,
    rejected_at INTEGER NOT NULL                          -- unix ms
);

ALTER TABLE history ADD COLUMN edited_text TEXT;
