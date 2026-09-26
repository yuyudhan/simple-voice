-- FilePath: crates/sv-storage/migrations/0005_history_edits.sql
-- Edit mode rewrites the selected text by voice. An edit is a history row whose source_text holds
-- the selection it replaced; raw_text is the spoken instruction and final_text the edited text.
-- Dictations, including every row written before this, keep source_text NULL.

ALTER TABLE history ADD COLUMN source_text TEXT;
