-- File upload audit logging for security tracking
CREATE TABLE IF NOT EXISTS file_upload_audit (
    id UUID PRIMARY KEY DEFAULT gen_uuid_v7(),
    uploaded_by TEXT,
    filename TEXT NOT NULL,
    original_filename TEXT,
    file_size BIGINT,
    content_type TEXT,
    blocked BOOLEAN NOT NULL DEFAULT FALSE,
    block_reason TEXT,
    detected_type TEXT,
    outcome TEXT NOT NULL,  -- 'accepted', 'blocked', 'quarantined'
    client_ip TEXT,
    user_agent TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_file_upload_audit_created ON file_upload_audit(created_at);
CREATE INDEX idx_file_upload_audit_blocked ON file_upload_audit(blocked) WHERE blocked = true;
CREATE INDEX idx_file_upload_audit_user ON file_upload_audit(uploaded_by);

COMMENT ON TABLE file_upload_audit IS 'Audit log for all file upload attempts, especially blocked ones';
