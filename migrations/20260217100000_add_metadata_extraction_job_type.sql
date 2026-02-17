-- Add metadata_extraction to job_type enum.
-- MetadataExtractionHandler was added in #430 but the enum value was never migrated.
ALTER TYPE job_type ADD VALUE IF NOT EXISTS 'metadata_extraction';
