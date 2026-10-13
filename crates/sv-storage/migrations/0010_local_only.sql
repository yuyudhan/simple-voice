-- FilePath: crates/sv-storage/migrations/0010_local_only.sql
-- Simple Voice runs only on the Mac: Groq and custom OpenAI-compatible endpoints are gone.
-- Saved selections of them move to local choices; their credentials and endpoint settings are
-- deleted. History, the dictionary and every other setting are kept as they are; history rows
-- recorded by the removed providers keep their model ids.

-- Groq Whisper → Smart Select, which runs the best downloaded local model for the languages.
UPDATE settings SET value = '"smart-select"'
WHERE key = 'transcriptionModel' AND value = '"groq-whisper"';

-- Groq or custom formatting → Apple Intelligence on-device (the new default).
UPDATE settings SET value = '"apple"'
WHERE key = 'postProcessing' AND value IN ('"groq"', '"custom"');

DELETE FROM settings
WHERE key IN ('groqApiKey', 'customApiKey', 'groqFormattingModel', 'customBaseUrl', 'customModel');

-- Where a dictation's style came from: 'app' | 'category' | 'global'; NULL for older rows.
ALTER TABLE history ADD COLUMN style_source TEXT;
