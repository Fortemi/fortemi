-- no-transaction
-- Index-backed lexical candidates: matric_simple attachment text expression
-- index for the simple (non-stemming / non-Latin) lexical strategy.
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_attachment_extracted_fts_simple
    ON public.attachment USING gin (to_tsvector('public.matric_simple', COALESCE(extracted_text, '')));
