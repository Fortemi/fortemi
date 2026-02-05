-- Add parent_id to collection table for nested folder hierarchy

ALTER TABLE collection ADD COLUMN parent_id UUID REFERENCES collection(id) ON DELETE SET NULL;

-- Index for listing children of a collection
CREATE INDEX idx_collection_parent ON collection(parent_id);
