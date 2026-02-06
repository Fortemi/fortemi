-- Add document_type_inference job type for async AI-based document classification.
-- Phase 2 of two-phase detection: extraction strategy is set synchronously from MIME type,
-- document type classification happens asynchronously after content extraction.
DO $$ BEGIN
    IF NOT EXISTS (SELECT 1 FROM pg_enum
                   WHERE enumlabel = 'document_type_inference'
                   AND enumtypid = 'job_type'::regtype) THEN
        ALTER TYPE job_type ADD VALUE 'document_type_inference';
    END IF;
END$$;
