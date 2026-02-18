-- Add graph_maintenance job type for the graph quality maintenance pipeline (#482).
-- Runs normalization, SNN, PFNET, and Louvain as a single orchestrated job.
ALTER TYPE job_type ADD VALUE IF NOT EXISTS 'graph_maintenance';
