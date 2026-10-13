-- FilePath: crates/sv-storage/migrations/0011_snippets.sql
-- Personal snippets: a spoken trigger phrase that dictation expands into saved text.

CREATE TABLE snippets (
    id             INTEGER PRIMARY KEY NOT NULL,
    trigger_phrase TEXT NOT NULL COLLATE NOCASE UNIQUE,  -- matched ignoring case
    body           TEXT NOT NULL,                       -- pasted verbatim, may be multiline
    created_at     INTEGER NOT NULL,                    -- unix ms
    updated_at     INTEGER NOT NULL                     -- unix ms
);
