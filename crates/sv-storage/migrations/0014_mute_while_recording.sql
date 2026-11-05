-- FilePath: crates/sv-storage/migrations/0014_mute_while_recording.sql
-- "Mute all audio while dictating" (off by default) became "Mute speakers while recording", on by
-- default for every install. The old row is deleted so existing installs take the new default
-- too; a missing row takes `Settings::default()`.

DELETE FROM settings WHERE key = 'muteWhileDictating';
