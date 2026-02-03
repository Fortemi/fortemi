-- Seed Research Document Types Migration
-- Implements specialized document types for research/scientific discovery workflows
-- Reference: Issue #411

-- ============================================================================
-- Research Document Type Seeding
-- ============================================================================

-- REF: Reference Card (Academic paper summaries)
-- Chunking: per_section for structured academic content
-- Detection: REF-*.md files or <!-- Document Type: reference-card --> markers
INSERT INTO document_type (
    name,
    display_name,
    category,
    description,
    file_extensions,
    filename_patterns,
    magic_patterns,
    chunking_strategy,
    chunk_size_default,
    chunk_overlap_default,
    preserve_boundaries,
    content_types,
    is_system,
    is_active
) VALUES (
    'research/reference',
    'Reference Card',
    'research',
    'Academic paper reference cards with structured summaries, citations, and key findings',
    ARRAY['.md'],
    ARRAY['REF-*.md', 'REF-[0-9]*.md'],
    ARRAY['<!-- Document Type: reference-card -->'],
    'per_section',
    1500,
    200,
    TRUE,
    ARRAY['prose', 'technical', 'academic'],
    TRUE,
    TRUE
);

-- LIT: Literature Review
-- Chunking: per_section for thematic analysis and source summaries
-- Detection: LIT-*.md files or <!-- Document Type: literature-review --> markers
INSERT INTO document_type (
    name,
    display_name,
    category,
    description,
    file_extensions,
    filename_patterns,
    magic_patterns,
    chunking_strategy,
    chunk_size_default,
    chunk_overlap_default,
    preserve_boundaries,
    content_types,
    is_system,
    is_active
) VALUES (
    'research/literature-review',
    'Literature Review',
    'research',
    'Systematic literature reviews with thematic analysis and synthesis',
    ARRAY['.md'],
    ARRAY['LIT-*.md', 'LIT-[0-9]*.md'],
    ARRAY['<!-- Document Type: literature-review -->'],
    'per_section',
    1500,
    200,
    TRUE,
    ARRAY['prose', 'academic'],
    TRUE,
    TRUE
);

-- EXP: Experiment Log
-- Chunking: per_section for structured experimental records
-- Detection: EXP-*.md files or <!-- Document Type: experiment-log --> markers
INSERT INTO document_type (
    name,
    display_name,
    category,
    description,
    file_extensions,
    filename_patterns,
    magic_patterns,
    chunking_strategy,
    chunk_size_default,
    chunk_overlap_default,
    preserve_boundaries,
    content_types,
    is_system,
    is_active
) VALUES (
    'research/experiment',
    'Experiment Log',
    'research',
    'Structured experiment logs with setup, methodology, observations, and results',
    ARRAY['.md'],
    ARRAY['EXP-*.md', 'EXP-[0-9]*.md'],
    ARRAY['<!-- Document Type: experiment-log -->'],
    'per_section',
    1500,
    200,
    TRUE,
    ARRAY['prose', 'technical', 'academic'],
    TRUE,
    TRUE
);

-- DISC: Discovery Note
-- Chunking: whole document for atomic insights
-- Detection: DISC-*.md files or <!-- Document Type: discovery-note --> markers
INSERT INTO document_type (
    name,
    display_name,
    category,
    description,
    file_extensions,
    filename_patterns,
    magic_patterns,
    chunking_strategy,
    chunk_size_default,
    chunk_overlap_default,
    preserve_boundaries,
    content_types,
    is_system,
    is_active
) VALUES (
    'research/discovery',
    'Discovery Note',
    'research',
    'Quick-capture discovery notes for insights, observations, and connections',
    ARRAY['.md'],
    ARRAY['DISC-*.md', 'DISC-[0-9]*.md'],
    ARRAY['<!-- Document Type: discovery-note -->'],
    'whole',
    NULL,
    NULL,
    TRUE,
    ARRAY['prose'],
    TRUE,
    TRUE
);

-- RQ: Research Question
-- Chunking: whole document for atomic research questions
-- Detection: RQ-*.md files or <!-- Document Type: research-question --> markers
INSERT INTO document_type (
    name,
    display_name,
    category,
    description,
    file_extensions,
    filename_patterns,
    magic_patterns,
    chunking_strategy,
    chunk_size_default,
    chunk_overlap_default,
    preserve_boundaries,
    content_types,
    is_system,
    is_active
) VALUES (
    'research/question',
    'Research Question',
    'research',
    'Research questions with context, rationale, and related work',
    ARRAY['.md'],
    ARRAY['RQ-*.md', 'RQ-[0-9]*.md'],
    ARRAY['<!-- Document Type: research-question -->'],
    'whole',
    NULL,
    NULL,
    TRUE,
    ARRAY['prose', 'academic'],
    TRUE,
    TRUE
);

-- HYP: Hypothesis Card
-- Chunking: whole document for atomic hypothesis statements
-- Detection: HYP-*.md files or <!-- Document Type: hypothesis-card --> markers
INSERT INTO document_type (
    name,
    display_name,
    category,
    description,
    file_extensions,
    filename_patterns,
    magic_patterns,
    chunking_strategy,
    chunk_size_default,
    chunk_overlap_default,
    preserve_boundaries,
    content_types,
    is_system,
    is_active
) VALUES (
    'research/hypothesis',
    'Hypothesis Card',
    'research',
    'Hypothesis statements with testable predictions and expected outcomes',
    ARRAY['.md'],
    ARRAY['HYP-*.md', 'HYP-[0-9]*.md'],
    ARRAY['<!-- Document Type: hypothesis-card -->'],
    'whole',
    NULL,
    NULL,
    TRUE,
    ARRAY['prose', 'academic'],
    TRUE,
    TRUE
);

-- PROT: Protocol
-- Chunking: per_section for step-by-step procedures
-- Detection: PROT-*.md files or <!-- Document Type: protocol --> markers
INSERT INTO document_type (
    name,
    display_name,
    category,
    description,
    file_extensions,
    filename_patterns,
    magic_patterns,
    chunking_strategy,
    chunk_size_default,
    chunk_overlap_default,
    preserve_boundaries,
    content_types,
    is_system,
    is_active
) VALUES (
    'research/protocol',
    'Protocol',
    'research',
    'Detailed research protocols and standard operating procedures',
    ARRAY['.md'],
    ARRAY['PROT-*.md', 'PROT-[0-9]*.md'],
    ARRAY['<!-- Document Type: protocol -->'],
    'per_section',
    1500,
    200,
    TRUE,
    ARRAY['prose', 'technical'],
    TRUE,
    TRUE
);

-- DATA: Data Dictionary
-- Chunking: per_section for structured dataset documentation
-- Detection: DATA-*.md files or <!-- Document Type: data-dictionary --> markers
INSERT INTO document_type (
    name,
    display_name,
    category,
    description,
    file_extensions,
    filename_patterns,
    magic_patterns,
    chunking_strategy,
    chunk_size_default,
    chunk_overlap_default,
    preserve_boundaries,
    content_types,
    is_system,
    is_active
) VALUES (
    'research/data-dictionary',
    'Data Dictionary',
    'research',
    'Dataset documentation with schema definitions, field descriptions, and metadata',
    ARRAY['.md'],
    ARRAY['DATA-*.md', 'DATA-[0-9]*.md'],
    ARRAY['<!-- Document Type: data-dictionary -->'],
    'per_section',
    1500,
    200,
    TRUE,
    ARRAY['prose', 'technical', 'data'],
    TRUE,
    TRUE
);

-- ============================================================================
-- Comments and Usage Notes
-- ============================================================================

COMMENT ON COLUMN document_type.filename_patterns IS
'Glob-style patterns for automatic type detection. Patterns support:
- Wildcards: REF-*.md matches REF-001.md, REF-attention.md
- Character classes: REF-[0-9]*.md matches only numeric IDs
Research documents follow ID-based naming: {PREFIX}-{NUMBER|SLUG}: {TITLE}';

COMMENT ON COLUMN document_type.magic_patterns IS
'Content-based detection via HTML comment markers in document frontmatter.
Research templates use: <!-- Document Type: {type-slug} -->
This enables detection even when filenames don''t follow naming conventions.';

COMMENT ON COLUMN document_type.chunking_strategy IS
'Research document chunking strategies:
- whole: Atomic documents (DISC, RQ, HYP) kept as single semantic units
- per_section: Structured documents (REF, LIT, EXP, PROT, DATA) split by section headings
This preserves document structure while enabling fine-grained retrieval.';

COMMENT ON COLUMN document_type.content_types IS
'Content type tags for embedding selection:
- academic: Peer-reviewed research, citations, formal methodology
- technical: Procedures, specifications, data schemas
- prose: Natural language narrative content
- data: Structured data descriptions
These tags guide embedding config selection for optimal semantic search.';
