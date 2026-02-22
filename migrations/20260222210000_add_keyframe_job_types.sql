-- Add keyframe_vision and keyframe_assembly job types for atomic per-frame vision (#526)
ALTER TYPE job_type ADD VALUE IF NOT EXISTS 'keyframe_vision';
ALTER TYPE job_type ADD VALUE IF NOT EXISTS 'keyframe_assembly';
