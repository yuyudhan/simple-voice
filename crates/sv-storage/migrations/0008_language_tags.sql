-- FilePath: crates/sv-storage/migrations/0008_language_tags.sql
-- Dictation languages are BCP 47 tags now: romanised Hindi is `hi-Latn`, a script variant of
-- `hi`, instead of the app-private code `hinglish`. The fallback language never held it.

UPDATE settings SET value = REPLACE(value, '"hinglish"', '"hi-Latn"') WHERE key = 'languages';
