-- no-transaction
-- Index-backed lexical candidates: matric_english attachment text expression
-- index. idx_attachment_extracted_fts uses the unqualified built-in 'english'
-- config, which tokenizes differently from public.matric_english (no unicode
-- normalization), so it cannot serve matric_english predicates without changing
-- results. One statement per migration so CONCURRENTLY is legal.
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_attachment_extracted_fts_english
    ON public.attachment USING gin (to_tsvector('public.matric_english', COALESCE(extracted_text, '')));
