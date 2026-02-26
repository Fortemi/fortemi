-- Add ViewVision and ViewAssembly job types for atomic 3D model view processing (#533)
ALTER TYPE job_type ADD VALUE IF NOT EXISTS 'view_vision';
ALTER TYPE job_type ADD VALUE IF NOT EXISTS 'view_assembly';
