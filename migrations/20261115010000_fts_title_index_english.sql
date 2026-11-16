-- no-transaction
-- Index-backed lexical candidates: matric_english title expression index.
-- Search predicates use to_tsvector('public.matric_english', COALESCE(title, ''))
-- so title matches are served by this GIN index instead of re-parsing every
-- note title at query time. Built CONCURRENTLY (one statement per migration:
-- a multi-statement simple query would run inside an implicit transaction).
-- Archive schemas are handled by 20261115040000.
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_note_title_tsv_english
    ON public.note USING gin (to_tsvector('public.matric_english', COALESCE(title, '')));
