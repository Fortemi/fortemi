-- Make the SKOS embedding-change trigger restorable with pgvector.
--
-- pg_dump serializes the trigger condition, and pg_restore cannot recreate
-- `vector IS DISTINCT FROM vector` because pgvector does not define vector
-- equality. Comparing the text representation preserves change detection for
-- this trigger and makes pre-migration recovery dumps restorable.
DO $$
BEGIN
  IF to_regclass('public.skos_concept') IS NULL
     OR to_regprocedure('public.queue_reembed_for_skos_changes()') IS NULL THEN
    RETURN;
  END IF;

  DROP TRIGGER IF EXISTS trg_reembed_on_skos_concept_update ON public.skos_concept;
  CREATE TRIGGER trg_reembed_on_skos_concept_update
  AFTER UPDATE ON public.skos_concept
  FOR EACH ROW
  WHEN (OLD.embedding::text IS DISTINCT FROM NEW.embedding::text)
  EXECUTE FUNCTION public.queue_reembed_for_skos_changes();
END $$;
