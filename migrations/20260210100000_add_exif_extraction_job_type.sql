-- Add exif_extraction to the job_type enum
-- This enables the ExifExtractionHandler to process EXIF metadata extraction jobs
ALTER TYPE job_type ADD VALUE IF NOT EXISTS 'exif_extraction';
