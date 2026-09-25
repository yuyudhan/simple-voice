-- FilePath: crates/sv-storage/migrations/0002_not_pasted_status.sql
-- Adds the 'not_pasted' history status. Before it existed, a dictation whose paste failed was
-- stored as 'pasted' or 'unformatted' with the failure in `error`; no other path set `error` on
-- those statuses, so such rows are exactly the ones that were never pasted.

UPDATE history
SET status = 'not_pasted'
WHERE status IN ('pasted', 'unformatted') AND error IS NOT NULL;
