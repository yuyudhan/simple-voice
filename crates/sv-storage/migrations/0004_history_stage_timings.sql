-- FilePath: crates/sv-storage/migrations/0004_history_stage_timings.sql
-- Records how long transcription and the formatting pass took, so each model's speed can be
-- compared. Rows written before this stay NULL: their stage timings were never measured.

ALTER TABLE history ADD COLUMN transcribe_ms INTEGER;
ALTER TABLE history ADD COLUMN format_ms INTEGER;
