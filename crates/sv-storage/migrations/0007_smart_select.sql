-- FilePath: crates/sv-storage/migrations/0007_smart_select.sql
-- Smart Select may run a second model when the first one errors or is unsure. model keeps the
-- model that produced raw_text; first_model is the model tried first and retry_reason why the
-- second ran ('failed' or 'low_confidence'). Both stay NULL when no retry happened, including
-- every row written before this.

ALTER TABLE history ADD COLUMN first_model TEXT;
ALTER TABLE history ADD COLUMN retry_reason TEXT;
