-- FilePath: crates/sv-storage/migrations/0012_meetings.sql
-- Personal meetings: local recordings, their timestamped transcript split by speaker, and what
-- the on-device language model wrote about them.

CREATE TABLE meetings (
    id           INTEGER PRIMARY KEY NOT NULL,
    title        TEXT NOT NULL,
    started_at   INTEGER NOT NULL,              -- unix ms
    ended_at     INTEGER,                       -- unix ms; NULL while recording
    duration_ms  INTEGER NOT NULL DEFAULT 0,
    status       TEXT NOT NULL,                 -- 'recording' | 'processing' | 'ready' | 'failed' | 'cancelled'
    microphone   INTEGER NOT NULL,              -- 0 | 1
    system_audio INTEGER NOT NULL,              -- 0 | 1
    language     TEXT,
    error        TEXT,
    audio_dir    TEXT NOT NULL,                 -- folder holding the meeting's WAV files
    model        TEXT                           -- transcription model id that produced the segments
);
CREATE INDEX meetings_started_at ON meetings(started_at);

CREATE TABLE meeting_speakers (
    id         INTEGER PRIMARY KEY NOT NULL,
    meeting_id INTEGER NOT NULL REFERENCES meetings(id) ON DELETE CASCADE,
    label      TEXT NOT NULL,                   -- 'Speaker 1', in order of first appearance
    name       TEXT                             -- set by the user
);
CREATE INDEX meeting_speakers_meeting ON meeting_speakers(meeting_id);

CREATE TABLE meeting_segments (
    id         INTEGER PRIMARY KEY NOT NULL,
    meeting_id INTEGER NOT NULL REFERENCES meetings(id) ON DELETE CASCADE,
    start_ms   INTEGER NOT NULL,
    end_ms     INTEGER NOT NULL,
    speaker_id INTEGER REFERENCES meeting_speakers(id) ON DELETE SET NULL,
    text       TEXT NOT NULL
);
CREATE INDEX meeting_segments_meeting ON meeting_segments(meeting_id, start_ms);

CREATE TABLE meeting_summaries (
    meeting_id   INTEGER PRIMARY KEY NOT NULL REFERENCES meetings(id) ON DELETE CASCADE,
    body         TEXT NOT NULL,
    action_items TEXT NOT NULL,                 -- JSON array of ActionItem
    model        TEXT NOT NULL,
    created_at   INTEGER NOT NULL
);

CREATE TABLE meeting_chat (
    id         INTEGER PRIMARY KEY NOT NULL,
    meeting_id INTEGER NOT NULL REFERENCES meetings(id) ON DELETE CASCADE,
    role       TEXT NOT NULL,                   -- 'user' | 'assistant'
    body       TEXT NOT NULL,
    citations  TEXT NOT NULL,                   -- JSON array of Citation
    created_at INTEGER NOT NULL
);
CREATE INDEX meeting_chat_meeting ON meeting_chat(meeting_id, id);

CREATE TABLE meeting_translations (
    id         INTEGER PRIMARY KEY NOT NULL,
    meeting_id INTEGER NOT NULL REFERENCES meetings(id) ON DELETE CASCADE,
    language   TEXT NOT NULL,
    model      TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    UNIQUE (meeting_id, language)
);

CREATE TABLE meeting_translation_segments (
    translation_id INTEGER NOT NULL REFERENCES meeting_translations(id) ON DELETE CASCADE,
    segment_id     INTEGER NOT NULL REFERENCES meeting_segments(id) ON DELETE CASCADE,
    body           TEXT NOT NULL,
    PRIMARY KEY (translation_id, segment_id)
);
