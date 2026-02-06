-- Backfill MIME types for all document types
-- This migration adds MIME type mappings to document types that don't have them yet.
-- Document types that already have MIME types (from seed_media_document_types) are skipped.

-- Core Types (from seed_core_document_types)
UPDATE document_type SET mime_types = ARRAY['text/markdown', 'text/x-markdown']::TEXT[]
WHERE name = 'markdown' AND (mime_types IS NULL OR mime_types = '{}');

UPDATE document_type SET mime_types = ARRAY['text/plain']::TEXT[]
WHERE name = 'plaintext' AND (mime_types IS NULL OR mime_types = '{}');

UPDATE document_type SET mime_types = ARRAY['text/x-rust']::TEXT[]
WHERE name = 'rust' AND (mime_types IS NULL OR mime_types = '{}');

UPDATE document_type SET mime_types = ARRAY['text/x-python', 'application/x-python']::TEXT[]
WHERE name = 'python' AND (mime_types IS NULL OR mime_types = '{}');

UPDATE document_type SET mime_types = ARRAY['text/typescript', 'application/typescript']::TEXT[]
WHERE name = 'typescript' AND (mime_types IS NULL OR mime_types = '{}');

UPDATE document_type SET mime_types = ARRAY['text/javascript', 'application/javascript']::TEXT[]
WHERE name = 'javascript' AND (mime_types IS NULL OR mime_types = '{}');

UPDATE document_type SET mime_types = ARRAY['text/x-go']::TEXT[]
WHERE name = 'go' AND (mime_types IS NULL OR mime_types = '{}');

UPDATE document_type SET mime_types = ARRAY['text/x-java', 'text/x-java-source']::TEXT[]
WHERE name = 'java' AND (mime_types IS NULL OR mime_types = '{}');

UPDATE document_type SET mime_types = ARRAY['application/json']::TEXT[]
WHERE name = 'json' AND (mime_types IS NULL OR mime_types = '{}');

UPDATE document_type SET mime_types = ARRAY['application/x-yaml', 'text/yaml']::TEXT[]
WHERE name = 'yaml' AND (mime_types IS NULL OR mime_types = '{}');

UPDATE document_type SET mime_types = ARRAY['application/toml']::TEXT[]
WHERE name = 'toml' AND (mime_types IS NULL OR mime_types = '{}');

UPDATE document_type SET mime_types = ARRAY['text/html']::TEXT[]
WHERE name = 'html' AND (mime_types IS NULL OR mime_types = '{}');

UPDATE document_type SET mime_types = ARRAY['application/xml', 'text/xml']::TEXT[]
WHERE name = 'xml' AND (mime_types IS NULL OR mime_types = '{}');

UPDATE document_type SET mime_types = ARRAY['application/sql', 'text/x-sql']::TEXT[]
WHERE name = 'sql' AND (mime_types IS NULL OR mime_types = '{}');

-- Technical Types (from seed_technical_document_types)
UPDATE document_type SET mime_types = ARRAY['application/vnd.oai.openapi+json', 'application/vnd.oai.openapi']::TEXT[]
WHERE name = 'openapi' AND (mime_types IS NULL OR mime_types = '{}');

UPDATE document_type SET mime_types = ARRAY['application/graphql']::TEXT[]
WHERE name = 'graphql-schema' AND (mime_types IS NULL OR mime_types = '{}');

UPDATE document_type SET mime_types = ARRAY['text/x-protobuf']::TEXT[]
WHERE name = 'protobuf' AND (mime_types IS NULL OR mime_types = '{}');

UPDATE document_type SET mime_types = ARRAY['application/vnd.aai.asyncapi+json']::TEXT[]
WHERE name = 'asyncapi' AND (mime_types IS NULL OR mime_types = '{}');

UPDATE document_type SET mime_types = ARRAY['application/schema+json']::TEXT[]
WHERE name = 'json-schema' AND (mime_types IS NULL OR mime_types = '{}');

UPDATE document_type SET mime_types = ARRAY['application/x-hcl', 'text/x-hcl']::TEXT[]
WHERE name = 'terraform' AND (mime_types IS NULL OR mime_types = '{}');

UPDATE document_type SET mime_types = ARRAY['application/x-yaml']::TEXT[]
WHERE name = 'kubernetes' AND (mime_types IS NULL OR mime_types = '{}');

UPDATE document_type SET mime_types = ARRAY['text/x-dockerfile']::TEXT[]
WHERE name = 'dockerfile' AND (mime_types IS NULL OR mime_types = '{}');

UPDATE document_type SET mime_types = ARRAY['application/x-yaml']::TEXT[]
WHERE name = 'docker-compose' AND (mime_types IS NULL OR mime_types = '{}');

UPDATE document_type SET mime_types = ARRAY['application/x-yaml']::TEXT[]
WHERE name = 'ansible' AND (mime_types IS NULL OR mime_types = '{}');

UPDATE document_type SET mime_types = ARRAY['application/x-yaml']::TEXT[]
WHERE name = 'cloudformation' AND (mime_types IS NULL OR mime_types = '{}');

UPDATE document_type SET mime_types = ARRAY['application/x-yaml']::TEXT[]
WHERE name = 'helm' AND (mime_types IS NULL OR mime_types = '{}');

UPDATE document_type SET mime_types = ARRAY['application/sql']::TEXT[]
WHERE name = 'sql-migration' AND (mime_types IS NULL OR mime_types = '{}');

UPDATE document_type SET mime_types = ARRAY['text/x-prisma']::TEXT[]
WHERE name = 'prisma' AND (mime_types IS NULL OR mime_types = '{}');

UPDATE document_type SET mime_types = ARRAY['text/typescript']::TEXT[]
WHERE name = 'drizzle' AND (mime_types IS NULL OR mime_types = '{}');

UPDATE document_type SET mime_types = ARRAY['text/x-python']::TEXT[]
WHERE name = 'sqlalchemy' AND (mime_types IS NULL OR mime_types = '{}');

UPDATE document_type SET mime_types = ARRAY['text/x-mermaid', 'text/x-plantuml']::TEXT[]
WHERE name = 'erd' AND (mime_types IS NULL OR mime_types = '{}');

UPDATE document_type SET mime_types = ARRAY['application/x-shellscript', 'text/x-shellscript']::TEXT[]
WHERE name = 'bash' AND (mime_types IS NULL OR mime_types = '{}');

UPDATE document_type SET mime_types = ARRAY['application/x-shellscript', 'text/x-shellscript']::TEXT[]
WHERE name = 'zsh' AND (mime_types IS NULL OR mime_types = '{}');

UPDATE document_type SET mime_types = ARRAY['application/x-powershell']::TEXT[]
WHERE name = 'powershell' AND (mime_types IS NULL OR mime_types = '{}');

UPDATE document_type SET mime_types = ARRAY['text/x-makefile']::TEXT[]
WHERE name = 'makefile' AND (mime_types IS NULL OR mime_types = '{}');

UPDATE document_type SET mime_types = ARRAY['text/x-justfile']::TEXT[]
WHERE name = 'justfile' AND (mime_types IS NULL OR mime_types = '{}');

UPDATE document_type SET mime_types = ARRAY['text/x-cmake']::TEXT[]
WHERE name = 'cmake' AND (mime_types IS NULL OR mime_types = '{}');

UPDATE document_type SET mime_types = ARRAY['text/x-gradle']::TEXT[]
WHERE name = 'gradle' AND (mime_types IS NULL OR mime_types = '{}');

UPDATE document_type SET mime_types = ARRAY['text/x-rst', 'text/prs.fallenstein.rst']::TEXT[]
WHERE name = 'rst' AND (mime_types IS NULL OR mime_types = '{}');

UPDATE document_type SET mime_types = ARRAY['text/asciidoc']::TEXT[]
WHERE name = 'asciidoc' AND (mime_types IS NULL OR mime_types = '{}');

UPDATE document_type SET mime_types = ARRAY['text/x-org']::TEXT[]
WHERE name = 'org-mode' AND (mime_types IS NULL OR mime_types = '{}');

UPDATE document_type SET mime_types = ARRAY['application/x-latex', 'text/x-latex']::TEXT[]
WHERE name = 'latex' AND (mime_types IS NULL OR mime_types = '{}');

UPDATE document_type SET mime_types = ARRAY['text/troff', 'application/x-troff-man']::TEXT[]
WHERE name = 'man-page' AND (mime_types IS NULL OR mime_types = '{}');

UPDATE document_type SET mime_types = ARRAY['application/x-ipynb+json']::TEXT[]
WHERE name = 'jupyter' AND (mime_types IS NULL OR mime_types = '{}');

UPDATE document_type SET mime_types = ARRAY['text/mdx']::TEXT[]
WHERE name = 'mdx' AND (mime_types IS NULL OR mime_types = '{}');

UPDATE document_type SET mime_types = ARRAY['text/plain']::TEXT[]
WHERE name = 'docstring' AND (mime_types IS NULL OR mime_types = '{}');

UPDATE document_type SET mime_types = ARRAY['application/toml']::TEXT[]
WHERE name = 'cargo-toml' AND (mime_types IS NULL OR mime_types = '{}');

UPDATE document_type SET mime_types = ARRAY['application/json']::TEXT[]
WHERE name = 'package-json' AND (mime_types IS NULL OR mime_types = '{}');

UPDATE document_type SET mime_types = ARRAY['application/toml']::TEXT[]
WHERE name = 'pyproject' AND (mime_types IS NULL OR mime_types = '{}');

UPDATE document_type SET mime_types = ARRAY['text/plain']::TEXT[]
WHERE name = 'go-mod' AND (mime_types IS NULL OR mime_types = '{}');

UPDATE document_type SET mime_types = ARRAY['application/xml']::TEXT[]
WHERE name = 'pom-xml' AND (mime_types IS NULL OR mime_types = '{}');

UPDATE document_type SET mime_types = ARRAY['text/x-ruby']::TEXT[]
WHERE name = 'gemfile' AND (mime_types IS NULL OR mime_types = '{}');

UPDATE document_type SET mime_types = ARRAY['text/plain']::TEXT[]
WHERE name = 'requirements' AND (mime_types IS NULL OR mime_types = '{}');

UPDATE document_type SET mime_types = ARRAY['application/json', 'text/plain']::TEXT[]
WHERE name = 'lockfile' AND (mime_types IS NULL OR mime_types = '{}');

UPDATE document_type SET mime_types = ARRAY['text/x-log', 'text/plain']::TEXT[]
WHERE name = 'log-file' AND (mime_types IS NULL OR mime_types = '{}');

UPDATE document_type SET mime_types = ARRAY['text/plain']::TEXT[]
WHERE name = 'stack-trace' AND (mime_types IS NULL OR mime_types = '{}');

UPDATE document_type SET mime_types = ARRAY['application/json']::TEXT[]
WHERE name = 'error-report' AND (mime_types IS NULL OR mime_types = '{}');

UPDATE document_type SET mime_types = ARRAY['text/plain']::TEXT[]
WHERE name = 'metrics' AND (mime_types IS NULL OR mime_types = '{}');

UPDATE document_type SET mime_types = ARRAY['application/json']::TEXT[]
WHERE name = 'trace-json' AND (mime_types IS NULL OR mime_types = '{}');

-- General Types (from seed_general_document_types)
UPDATE document_type SET mime_types = ARRAY['application/pdf', 'application/vnd.openxmlformats-officedocument.wordprocessingml.document']::TEXT[]
WHERE name = 'contract' AND (mime_types IS NULL OR mime_types = '{}');

UPDATE document_type SET mime_types = ARRAY['application/pdf', 'application/vnd.openxmlformats-officedocument.wordprocessingml.document', 'text/markdown']::TEXT[]
WHERE name = 'policy' AND (mime_types IS NULL OR mime_types = '{}');

UPDATE document_type SET mime_types = ARRAY['application/pdf', 'application/vnd.openxmlformats-officedocument.wordprocessingml.document', 'text/markdown']::TEXT[]
WHERE name = 'proposal' AND (mime_types IS NULL OR mime_types = '{}');

UPDATE document_type SET mime_types = ARRAY['application/pdf', 'application/vnd.openxmlformats-officedocument.spreadsheetml.sheet']::TEXT[]
WHERE name = 'invoice' AND (mime_types IS NULL OR mime_types = '{}');

UPDATE document_type SET mime_types = ARRAY['application/pdf', 'application/vnd.openxmlformats-officedocument.wordprocessingml.document']::TEXT[]
WHERE name = 'report' AND (mime_types IS NULL OR mime_types = '{}');

UPDATE document_type SET mime_types = ARRAY['application/pdf', 'application/vnd.openxmlformats-officedocument.wordprocessingml.document', 'text/markdown']::TEXT[]
WHERE name = 'sow' AND (mime_types IS NULL OR mime_types = '{}');

UPDATE document_type SET mime_types = ARRAY['application/pdf', 'application/vnd.openxmlformats-officedocument.wordprocessingml.document']::TEXT[]
WHERE name = 'nda' AND (mime_types IS NULL OR mime_types = '{}');

UPDATE document_type SET mime_types = ARRAY['application/pdf', 'text/html', 'text/markdown']::TEXT[]
WHERE name = 'terms-of-service' AND (mime_types IS NULL OR mime_types = '{}');

UPDATE document_type SET mime_types = ARRAY['message/rfc822', 'application/vnd.ms-outlook']::TEXT[]
WHERE name = 'email' AND (mime_types IS NULL OR mime_types = '{}');

UPDATE document_type SET mime_types = ARRAY['message/rfc822', 'application/mbox']::TEXT[]
WHERE name = 'email-thread' AND (mime_types IS NULL OR mime_types = '{}');

UPDATE document_type SET mime_types = ARRAY['text/plain', 'application/json']::TEXT[]
WHERE name = 'chat-log' AND (mime_types IS NULL OR mime_types = '{}');

UPDATE document_type SET mime_types = ARRAY['application/json']::TEXT[]
WHERE name = 'slack-export' AND (mime_types IS NULL OR mime_types = '{}');

UPDATE document_type SET mime_types = ARRAY['application/json', 'text/plain']::TEXT[]
WHERE name = 'discord-log' AND (mime_types IS NULL OR mime_types = '{}');

UPDATE document_type SET mime_types = ARRAY['text/markdown', 'application/vnd.openxmlformats-officedocument.wordprocessingml.document']::TEXT[]
WHERE name = 'meeting-notes' AND (mime_types IS NULL OR mime_types = '{}');

UPDATE document_type SET mime_types = ARRAY['text/plain', 'text/vtt', 'application/x-subrip']::TEXT[]
WHERE name = 'transcript' AND (mime_types IS NULL OR mime_types = '{}');

UPDATE document_type SET mime_types = ARRAY['text/markdown', 'text/plain']::TEXT[]
WHERE name = 'standup' AND (mime_types IS NULL OR mime_types = '{}');

UPDATE document_type SET mime_types = ARRAY['application/pdf', 'application/x-latex']::TEXT[]
WHERE name = 'academic-paper' AND (mime_types IS NULL OR mime_types = '{}');

UPDATE document_type SET mime_types = ARRAY['application/pdf', 'application/x-latex']::TEXT[]
WHERE name = 'arxiv' AND (mime_types IS NULL OR mime_types = '{}');

UPDATE document_type SET mime_types = ARRAY['application/pdf', 'application/xml']::TEXT[]
WHERE name = 'patent' AND (mime_types IS NULL OR mime_types = '{}');

UPDATE document_type SET mime_types = ARRAY['application/pdf', 'application/x-latex']::TEXT[]
WHERE name = 'thesis' AND (mime_types IS NULL OR mime_types = '{}');

UPDATE document_type SET mime_types = ARRAY['application/x-bibtex', 'application/x-research-info-systems']::TEXT[]
WHERE name = 'citation' AND (mime_types IS NULL OR mime_types = '{}');

UPDATE document_type SET mime_types = ARRAY['application/pdf', 'text/markdown']::TEXT[]
WHERE name = 'literature-review' AND (mime_types IS NULL OR mime_types = '{}');

UPDATE document_type SET mime_types = ARRAY['text/markdown', 'text/plain']::TEXT[]
WHERE name = 'research-note' AND (mime_types IS NULL OR mime_types = '{}');

UPDATE document_type SET mime_types = ARRAY['application/pdf', 'text/markdown']::TEXT[]
WHERE name = 'whitepaper' AND (mime_types IS NULL OR mime_types = '{}');

UPDATE document_type SET mime_types = ARRAY['text/markdown', 'text/html']::TEXT[]
WHERE name = 'blog-post' AND (mime_types IS NULL OR mime_types = '{}');

UPDATE document_type SET mime_types = ARRAY['text/markdown', 'text/html']::TEXT[]
WHERE name = 'article' AND (mime_types IS NULL OR mime_types = '{}');

UPDATE document_type SET mime_types = ARRAY['text/html', 'text/markdown']::TEXT[]
WHERE name = 'newsletter' AND (mime_types IS NULL OR mime_types = '{}');

UPDATE document_type SET mime_types = ARRAY['application/pdf', 'text/markdown']::TEXT[]
WHERE name = 'press-release' AND (mime_types IS NULL OR mime_types = '{}');

UPDATE document_type SET mime_types = ARRAY['text/plain', 'application/json']::TEXT[]
WHERE name = 'social-post' AND (mime_types IS NULL OR mime_types = '{}');

UPDATE document_type SET mime_types = ARRAY['text/plain', 'text/markdown']::TEXT[]
WHERE name = 'ad-copy' AND (mime_types IS NULL OR mime_types = '{}');

UPDATE document_type SET mime_types = ARRAY['text/plain', 'application/pdf']::TEXT[]
WHERE name = 'script' AND (mime_types IS NULL OR mime_types = '{}');

UPDATE document_type SET mime_types = ARRAY['text/markdown', 'application/vnd.openxmlformats-officedocument.wordprocessingml.document']::TEXT[]
WHERE name = 'book-chapter' AND (mime_types IS NULL OR mime_types = '{}');

UPDATE document_type SET mime_types = ARRAY['image/jpeg', 'image/png', 'image/gif', 'image/webp', 'image/svg+xml']::TEXT[]
WHERE name = 'image' AND (mime_types IS NULL OR mime_types = '{}');

UPDATE document_type SET mime_types = ARRAY['image/jpeg', 'image/png', 'application/pdf']::TEXT[]
WHERE name = 'image-with-text' AND (mime_types IS NULL OR mime_types = '{}');

UPDATE document_type SET mime_types = ARRAY['image/png', 'image/jpeg']::TEXT[]
WHERE name = 'screenshot' AND (mime_types IS NULL OR mime_types = '{}');

UPDATE document_type SET mime_types = ARRAY['image/svg+xml', 'image/png', 'application/x-drawio', 'application/x-excalidraw+json']::TEXT[]
WHERE name = 'diagram' AND (mime_types IS NULL OR mime_types = '{}');

UPDATE document_type SET mime_types = ARRAY['audio/mpeg', 'audio/wav', 'audio/x-m4a', 'audio/flac', 'audio/ogg']::TEXT[]
WHERE name = 'audio' AND (mime_types IS NULL OR mime_types = '{}');

UPDATE document_type SET mime_types = ARRAY['video/mp4', 'video/quicktime', 'video/x-msvideo', 'video/x-matroska', 'video/webm']::TEXT[]
WHERE name = 'video' AND (mime_types IS NULL OR mime_types = '{}');

UPDATE document_type SET mime_types = ARRAY['audio/mpeg', 'audio/x-m4a']::TEXT[]
WHERE name = 'podcast' AND (mime_types IS NULL OR mime_types = '{}');

UPDATE document_type SET mime_types = ARRAY['application/vnd.openxmlformats-officedocument.presentationml.presentation', 'application/vnd.ms-powerpoint', 'application/pdf']::TEXT[]
WHERE name = 'presentation' AND (mime_types IS NULL OR mime_types = '{}');

UPDATE document_type SET mime_types = ARRAY['text/markdown', 'text/plain']::TEXT[]
WHERE name = 'daily-note' AND (mime_types IS NULL OR mime_types = '{}');

UPDATE document_type SET mime_types = ARRAY['text/markdown', 'text/plain']::TEXT[]
WHERE name = 'journal' AND (mime_types IS NULL OR mime_types = '{}');

UPDATE document_type SET mime_types = ARRAY['text/html', 'application/json', 'text/markdown']::TEXT[]
WHERE name = 'bookmark' AND (mime_types IS NULL OR mime_types = '{}');

UPDATE document_type SET mime_types = ARRAY['text/markdown', 'text/plain', 'application/json']::TEXT[]
WHERE name = 'highlight' AND (mime_types IS NULL OR mime_types = '{}');

UPDATE document_type SET mime_types = ARRAY['text/markdown', 'text/plain', 'application/json']::TEXT[]
WHERE name = 'annotation' AND (mime_types IS NULL OR mime_types = '{}');

UPDATE document_type SET mime_types = ARRAY['text/markdown', 'text/plain']::TEXT[]
WHERE name = 'todo-list' AND (mime_types IS NULL OR mime_types = '{}');

UPDATE document_type SET mime_types = ARRAY['text/markdown', 'text/plain', 'text/html']::TEXT[]
WHERE name = 'recipe' AND (mime_types IS NULL OR mime_types = '{}');

UPDATE document_type SET mime_types = ARRAY['text/markdown', 'text/plain']::TEXT[]
WHERE name = 'reading-list' AND (mime_types IS NULL OR mime_types = '{}');

UPDATE document_type SET mime_types = ARRAY['text/csv']::TEXT[]
WHERE name = 'csv' AND (mime_types IS NULL OR mime_types = '{}');

UPDATE document_type SET mime_types = ARRAY['application/vnd.openxmlformats-officedocument.spreadsheetml.sheet', 'application/vnd.ms-excel']::TEXT[]
WHERE name = 'excel' AND (mime_types IS NULL OR mime_types = '{}');

UPDATE document_type SET mime_types = ARRAY['application/vnd.apache.parquet']::TEXT[]
WHERE name = 'parquet-schema' AND (mime_types IS NULL OR mime_types = '{}');

UPDATE document_type SET mime_types = ARRAY['application/avro', 'application/json']::TEXT[]
WHERE name = 'avro-schema' AND (mime_types IS NULL OR mime_types = '{}');

UPDATE document_type SET mime_types = ARRAY['application/xml', 'text/xml']::TEXT[]
WHERE name = 'xml-data' AND (mime_types IS NULL OR mime_types = '{}');

UPDATE document_type SET mime_types = ARRAY['application/x-ndjson']::TEXT[]
WHERE name = 'ndjson' AND (mime_types IS NULL OR mime_types = '{}');

UPDATE document_type SET mime_types = ARRAY['application/geo+json']::TEXT[]
WHERE name = 'geojson' AND (mime_types IS NULL OR mime_types = '{}');

UPDATE document_type SET mime_types = ARRAY['text/calendar']::TEXT[]
WHERE name = 'ical' AND (mime_types IS NULL OR mime_types = '{}');

-- Research Types (from seed_research_document_types)
UPDATE document_type SET mime_types = ARRAY['text/markdown']::TEXT[]
WHERE name = 'research/reference' AND (mime_types IS NULL OR mime_types = '{}');

UPDATE document_type SET mime_types = ARRAY['text/markdown']::TEXT[]
WHERE name = 'research/literature-review' AND (mime_types IS NULL OR mime_types = '{}');

UPDATE document_type SET mime_types = ARRAY['text/markdown']::TEXT[]
WHERE name = 'research/experiment' AND (mime_types IS NULL OR mime_types = '{}');

UPDATE document_type SET mime_types = ARRAY['text/markdown']::TEXT[]
WHERE name = 'research/discovery' AND (mime_types IS NULL OR mime_types = '{}');

UPDATE document_type SET mime_types = ARRAY['text/markdown']::TEXT[]
WHERE name = 'research/question' AND (mime_types IS NULL OR mime_types = '{}');

UPDATE document_type SET mime_types = ARRAY['text/markdown']::TEXT[]
WHERE name = 'research/hypothesis' AND (mime_types IS NULL OR mime_types = '{}');

UPDATE document_type SET mime_types = ARRAY['text/markdown']::TEXT[]
WHERE name = 'research/protocol' AND (mime_types IS NULL OR mime_types = '{}');

UPDATE document_type SET mime_types = ARRAY['text/markdown']::TEXT[]
WHERE name = 'research/data-dictionary' AND (mime_types IS NULL OR mime_types = '{}');

-- Agentic Types (from seed_agentic_document_types)
UPDATE document_type SET mime_types = ARRAY['text/plain', 'text/markdown']::TEXT[]
WHERE name = 'agent-prompt' AND (mime_types IS NULL OR mime_types = '{}');

UPDATE document_type SET mime_types = ARRAY['text/markdown']::TEXT[]
WHERE name = 'agent-skill' AND (mime_types IS NULL OR mime_types = '{}');

UPDATE document_type SET mime_types = ARRAY['application/x-yaml', 'application/json']::TEXT[]
WHERE name = 'agent-workflow' AND (mime_types IS NULL OR mime_types = '{}');

UPDATE document_type SET mime_types = ARRAY['application/json']::TEXT[]
WHERE name = 'mcp-tool' AND (mime_types IS NULL OR mime_types = '{}');

UPDATE document_type SET mime_types = ARRAY['text/markdown']::TEXT[]
WHERE name = 'rag-context' AND (mime_types IS NULL OR mime_types = '{}');

UPDATE document_type SET mime_types = ARRAY['application/json', 'text/markdown']::TEXT[]
WHERE name = 'ai-conversation' AND (mime_types IS NULL OR mime_types = '{}');

UPDATE document_type SET mime_types = ARRAY['application/x-ndjson', 'text/csv']::TEXT[]
WHERE name = 'fine-tune-data' AND (mime_types IS NULL OR mime_types = '{}');

UPDATE document_type SET mime_types = ARRAY['application/json']::TEXT[]
WHERE name = 'evaluation-set' AND (mime_types IS NULL OR mime_types = '{}');

-- Extraction Strategy Types (from seed_extraction_strategies)
UPDATE document_type SET mime_types = ARRAY['video/mp4', 'video/quicktime', 'video/webm']::TEXT[]
WHERE name = 'personal-memory' AND (mime_types IS NULL OR mime_types = '{}');

UPDATE document_type SET mime_types = ARRAY['audio/mpeg', 'audio/wav', 'audio/x-m4a', 'audio/webm']::TEXT[]
WHERE name = 'voice-memo' AND (mime_types IS NULL OR mime_types = '{}');

UPDATE document_type SET mime_types = ARRAY['application/pdf', 'image/jpeg', 'image/png', 'image/tiff']::TEXT[]
WHERE name = 'scanned-document' AND (mime_types IS NULL OR mime_types = '{}');

-- Temporal/Positional Types (from seed_temporal_positional_types)
UPDATE document_type SET mime_types = ARRAY['text/markdown', 'text/calendar']::TEXT[]
WHERE name = 'event' AND (mime_types IS NULL OR mime_types = '{}');

UPDATE document_type SET mime_types = ARRAY['text/markdown']::TEXT[]
WHERE name = 'meeting' AND (mime_types IS NULL OR mime_types = '{}');

UPDATE document_type SET mime_types = ARRAY['text/markdown']::TEXT[]
WHERE name = 'deadline' AND (mime_types IS NULL OR mime_types = '{}');

UPDATE document_type SET mime_types = ARRAY['text/markdown']::TEXT[]
WHERE name = 'milestone' AND (mime_types IS NULL OR mime_types = '{}');

UPDATE document_type SET mime_types = ARRAY['text/markdown']::TEXT[]
WHERE name = 'sprint-record' AND (mime_types IS NULL OR mime_types = '{}');

UPDATE document_type SET mime_types = ARRAY['text/markdown']::TEXT[]
WHERE name = 'weekly-review' AND (mime_types IS NULL OR mime_types = '{}');

UPDATE document_type SET mime_types = ARRAY['text/markdown']::TEXT[]
WHERE name = 'incident-report' AND (mime_types IS NULL OR mime_types = '{}');

UPDATE document_type SET mime_types = ARRAY['text/markdown']::TEXT[]
WHERE name = 'changelog-entry' AND (mime_types IS NULL OR mime_types = '{}');

UPDATE document_type SET mime_types = ARRAY['text/markdown']::TEXT[]
WHERE name = 'status-update' AND (mime_types IS NULL OR mime_types = '{}');

UPDATE document_type SET mime_types = ARRAY['text/markdown']::TEXT[]
WHERE name = 'retrospective' AND (mime_types IS NULL OR mime_types = '{}');

UPDATE document_type SET mime_types = ARRAY['text/markdown', 'application/geo+json']::TEXT[]
WHERE name = 'location-note' AND (mime_types IS NULL OR mime_types = '{}');

UPDATE document_type SET mime_types = ARRAY['text/markdown']::TEXT[]
WHERE name = 'travel-log' AND (mime_types IS NULL OR mime_types = '{}');

UPDATE document_type SET mime_types = ARRAY['text/markdown']::TEXT[]
WHERE name = 'site-survey' AND (mime_types IS NULL OR mime_types = '{}');

UPDATE document_type SET mime_types = ARRAY['text/markdown']::TEXT[]
WHERE name = 'field-note' AND (mime_types IS NULL OR mime_types = '{}');

UPDATE document_type SET mime_types = ARRAY['text/markdown', 'text/calendar']::TEXT[]
WHERE name = 'itinerary' AND (mime_types IS NULL OR mime_types = '{}');

UPDATE document_type SET mime_types = ARRAY['text/markdown']::TEXT[]
WHERE name = 'conference-session' AND (mime_types IS NULL OR mime_types = '{}');

UPDATE document_type SET mime_types = ARRAY['text/markdown']::TEXT[]
WHERE name = 'trip-entry' AND (mime_types IS NULL OR mime_types = '{}');

UPDATE document_type SET mime_types = ARRAY['text/markdown', 'text/calendar']::TEXT[]
WHERE name = 'availability' AND (mime_types IS NULL OR mime_types = '{}');
