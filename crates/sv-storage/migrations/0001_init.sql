-- FilePath: crates/sv-storage/migrations/0001_init.sql
-- Initial schema. Shipped migrations are never edited; later changes get a new file.

CREATE TABLE settings (
    key   TEXT PRIMARY KEY NOT NULL,
    value TEXT NOT NULL                -- JSON-encoded value
);

CREATE TABLE dictionary (
    id          INTEGER PRIMARY KEY NOT NULL,
    phrase      TEXT NOT NULL COLLATE NOCASE UNIQUE,  -- word to recognise / text heard
    replacement TEXT,                                 -- NULL = plain word; else written form
    created_at  INTEGER NOT NULL                      -- unix ms
);

CREATE TABLE history (
    id               INTEGER PRIMARY KEY NOT NULL,
    created_at       INTEGER NOT NULL,   -- unix ms, recording start
    status           TEXT NOT NULL,      -- 'pasted' | 'unformatted' | 'failed' | 'dropped'
    raw_text         TEXT NOT NULL,      -- transcript as returned by the model
    final_text       TEXT NOT NULL,      -- what was pasted (or would have been)
    error            TEXT,
    model            TEXT NOT NULL,      -- model id, e.g. 'groq-whisper'
    language         TEXT,
    style            TEXT NOT NULL,      -- 'formal' | 'casual'
    audio_ms         INTEGER NOT NULL,
    latency_ms       INTEGER NOT NULL,   -- stop -> paste
    word_count       INTEGER NOT NULL,
    dictionary_fixes INTEGER NOT NULL,   -- replacement-rule hits
    words_corrected  INTEGER NOT NULL,   -- word-level edit distance raw -> final
    app_name         TEXT,
    bundle_id        TEXT,
    app_category     TEXT NOT NULL,      -- see AppCategory
    audio_path       TEXT                -- set only while a failed entry can be retried
);

CREATE INDEX history_created_at ON history(created_at);
