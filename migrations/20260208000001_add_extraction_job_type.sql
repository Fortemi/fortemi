-- Add extraction job type for content extraction pipeline.
-- This job type handles the extraction of content from file attachments
-- using various strategies (text_native, pdf_text, code_ast, etc.)
DO $$ BEGIN
    IF NOT EXISTS (SELECT 1 FROM pg_enum
                   WHERE enumlabel = 'extraction'
                   AND enumtypid = 'job_type'::regtype) THEN
        ALTER TYPE job_type ADD VALUE 'extraction';
    END IF;
END$$;
