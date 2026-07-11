-- Drop stray archive-local clones of global/public tables (#1028).
--
-- Archive schema sync is deny-list based. These tables were added after the
-- shared-table list and may have been cloned into existing archive_* schemas
-- before they were marked global.

DO $$
DECLARE
  archive_schema text;
  table_name text;
  shared_tables text[] := ARRAY[
    'inference_config_audit',
    'archive_inference_override',
    'call_sessions',
    'transcript_segments',
    'incoming_webhook_receiver',
    'event_outbox',
    'inbound_source',
    'inbound_dlq'
  ];
BEGIN
  FOR archive_schema IN
    SELECT schema_name
    FROM public.archive_registry
    WHERE schema_name <> 'public'
  LOOP
    FOREACH table_name IN ARRAY shared_tables
    LOOP
      EXECUTE format('DROP TABLE IF EXISTS %I.%I CASCADE', archive_schema, table_name);
    END LOOP;
  END LOOP;
END $$;
