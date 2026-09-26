-- FilePath: crates/sv-storage/migrations/0003_history_format_model.sql
-- Records which model formatted a dictation. Rows written before this stay NULL: which model
-- (if any) formatted them was never stored.

ALTER TABLE history ADD COLUMN format_model TEXT;
