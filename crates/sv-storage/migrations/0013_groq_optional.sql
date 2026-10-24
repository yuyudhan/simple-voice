-- FilePath: crates/sv-storage/migrations/0013_groq_optional.sql
-- Groq returns as an opt-in provider beside the on-device models. Every existing install stays
-- on the Mac: it gets an explicit 'local' transcription provider row (a fresh database has no
-- rows and takes the default, which is local too), and any leftover row that still points at a
-- cloud choice moves to the same local choice migration 0010 made. The Groq API key lives in the
-- macOS Keychain only, so a key row in this table is deleted. History, the dictionary and every
-- other setting are kept as they are.

-- Existing installs keep transcribing on the Mac until the user picks Groq in Settings.
INSERT INTO settings (key, value)
SELECT 'transcriptionProvider', '"local"'
WHERE EXISTS (SELECT 1 FROM settings)
  AND NOT EXISTS (SELECT 1 FROM settings WHERE key = 'transcriptionProvider');

-- Groq Whisper is a provider now, never a voice model id; Smart Select is the local choice.
UPDATE settings SET value = '"smart-select"'
WHERE key = 'transcriptionModel' AND value = '"groq-whisper"';

-- Formatting and the meeting assistant stay on-device.
UPDATE settings SET value = '"apple"'
WHERE key = 'postProcessing' AND value IN ('"groq"', '"custom"');
UPDATE settings SET value = '"local"'
WHERE key = 'meetingAssistant' AND value = '"groq"';

DELETE FROM settings WHERE key IN ('groqApiKey', 'customApiKey');
