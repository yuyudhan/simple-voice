-- FilePath: crates/sv-storage/migrations/0009_parakeet_default.sql
-- The default voice model is now Parakeet TDT v3 instead of Groq Whisper. A missing settings row
-- takes the default, so an install that never saved a voice model would silently switch to a
-- model that is not downloaded. Any install that has saved a setting keeps Groq Whisper; a fresh
-- database has no settings rows and gets the new default.

INSERT INTO settings (key, value)
SELECT 'transcriptionModel', '"groq-whisper"'
WHERE EXISTS (SELECT 1 FROM settings)
  AND NOT EXISTS (SELECT 1 FROM settings WHERE key = 'transcriptionModel');
