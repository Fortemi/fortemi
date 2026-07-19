-- Canonical managed-attachment malware/content scan state (#970).
--
-- Existing rows remain unknown. Operators must explicitly rescan them or use
-- the documented local-only bypass; the migration never labels legacy bytes
-- clean without a scanner verdict.

DO $attachment_scan_state$
DECLARE
    target_schema TEXT;
BEGIN
    FOR target_schema IN
        SELECT 'public'
        UNION
        SELECT ar.schema_name
        FROM public.archive_registry AS ar
        WHERE ar.schema_name <> 'public'
    LOOP
        IF to_regclass(format('%I.attachment', target_schema)) IS NULL THEN
            CONTINUE;
        END IF;

        EXECUTE format(
            'ALTER TABLE %I.attachment
                ADD COLUMN IF NOT EXISTS virus_scan_status TEXT,
                ADD COLUMN IF NOT EXISTS virus_scan_at TIMESTAMPTZ,
                ADD COLUMN IF NOT EXISTS virus_scan_backend TEXT,
                ADD COLUMN IF NOT EXISTS virus_scan_engine_version TEXT,
                ADD COLUMN IF NOT EXISTS virus_scan_signature_version TEXT,
                ADD COLUMN IF NOT EXISTS virus_scan_reason_code TEXT,
                ADD COLUMN IF NOT EXISTS virus_scan_blob_hash TEXT',
            target_schema
        );
        EXECUTE format(
            $sql$
            UPDATE %I.attachment
            SET virus_scan_status = 'unknown'
            WHERE virus_scan_status IS NULL
               OR virus_scan_status NOT IN (
                   'unknown',
                   'pending',
                   'clean',
                   'infected',
                   'error',
                   'unsupported',
                   'bypassed'
               )
            $sql$,
            target_schema
        );
        EXECUTE format(
            'ALTER TABLE %I.attachment
                ALTER COLUMN virus_scan_status SET DEFAULT ''pending'',
                ALTER COLUMN virus_scan_status SET NOT NULL',
            target_schema
        );
        EXECUTE format(
            'ALTER TABLE %I.attachment
                DROP CONSTRAINT IF EXISTS attachment_virus_scan_status_check',
            target_schema
        );
        EXECUTE format(
            $sql$
            ALTER TABLE %I.attachment
                ADD CONSTRAINT attachment_virus_scan_status_check
                CHECK (
                    virus_scan_status IN (
                        'unknown',
                        'pending',
                        'clean',
                        'infected',
                        'error',
                        'unsupported',
                        'bypassed'
                    )
                )
            $sql$,
            target_schema
        );
        EXECUTE format(
            'CREATE INDEX IF NOT EXISTS idx_attachment_virus_scan_status
                ON %I.attachment (virus_scan_status)',
            target_schema
        );
        EXECUTE format(
            'COMMENT ON COLUMN %I.attachment.virus_scan_status IS
                ''Managed attachment scan verdict: unknown, pending, clean, infected, error, unsupported, or bypassed''',
            target_schema
        );
        EXECUTE format(
            'COMMENT ON COLUMN %I.attachment.virus_scan_reason_code IS
                ''Bounded Fortemi reason code; never raw scanner output, paths, URLs, or content''',
            target_schema
        );
    END LOOP;
END;
$attachment_scan_state$;
