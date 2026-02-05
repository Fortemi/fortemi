-- ============================================================================
-- Seed Media Document Type Templates Migration
-- Issue: #425 (proposed)
-- ============================================================================
-- Updates existing media doctypes with:
-- 1. requires_file_attachment = true
-- 2. extraction_strategy per media type
-- 3. auto_create_note = true
-- 4. note_template for AI-generated content
-- 5. agentic_config with embed_config
-- ============================================================================

-- ============================================================================
-- PART 1: IMAGE DOCTYPES
-- ============================================================================

-- Generic image
UPDATE document_type SET
    requires_file_attachment = TRUE,
    extraction_strategy = 'vision',
    auto_create_note = TRUE,
    note_template = E'# {{filename}}\n\n{{ai_description}}\n\n## Visual Details\n{{#if objects}}- **Objects:** {{objects}}{{/if}}\n{{#if text_content}}- **Text in Image:** {{text_content}}{{/if}}\n{{#if colors}}- **Colors:** {{colors}}{{/if}}\n\n## Metadata\n- **Dimensions:** {{dimensions}}\n- **Captured:** {{capture_date}}\n{{#if camera_model}}- **Camera:** {{camera_model}}{{/if}}\n{{#if gps_location}}- **Location:** {{gps_location}}{{/if}}\n\n---\n*Tags: {{suggested_tags}}*',
    embedding_model_override = 'clip-vit-b-32',
    agentic_config = '{
        "generation_prompt": "Describe this image in detail. Identify objects, people, text, and the overall scene or context.",
        "required_sections": [],
        "context_requirements": {"needs_vision_model": true},
        "agent_hints": {
            "extract_text": true,
            "identify_objects": true,
            "describe_colors": true
        },
        "embed_config": {
            "auto_embed": true,
            "use_clip": true,
            "model_override": "clip-vit-b-32",
            "priority": "normal"
        }
    }'::jsonb
WHERE name = 'image';

-- Screenshot
UPDATE document_type SET
    requires_file_attachment = TRUE,
    extraction_strategy = 'vision',
    auto_create_note = TRUE,
    note_template = E'# Screenshot: {{filename}}\n\n## Content\n{{ai_description}}\n\n## Text Extracted\n{{text_content}}\n\n## Details\n- **Application:** {{application}}\n- **Captured:** {{capture_date}}\n- **Dimensions:** {{dimensions}}\n\n---\n*Tags: {{suggested_tags}}*',
    embedding_model_override = 'clip-vit-b-32',
    agentic_config = '{
        "generation_prompt": "Describe this screenshot. Focus on the application shown, UI elements, and any visible text or data.",
        "context_requirements": {"needs_vision_model": true, "needs_ocr": true},
        "agent_hints": {
            "extract_text": true,
            "identify_application": true,
            "describe_ui_elements": true
        },
        "embed_config": {
            "auto_embed": true,
            "use_clip": true,
            "priority": "normal"
        }
    }'::jsonb
WHERE name = 'screenshot';

-- Diagram
UPDATE document_type SET
    requires_file_attachment = TRUE,
    extraction_strategy = 'vision',
    auto_create_note = TRUE,
    note_template = E'# {{title}}\n\n## Overview\n{{ai_description}}\n\n## Components\n{{components}}\n\n## Relationships\n{{relationships}}\n\n## Text Labels\n{{text_content}}\n\n---\n*Tags: {{suggested_tags}}*',
    agentic_config = '{
        "generation_prompt": "Analyze this diagram. Identify the type of diagram, its components, relationships between elements, and the overall concept it represents.",
        "context_requirements": {"needs_vision_model": true},
        "agent_hints": {
            "identify_diagram_type": true,
            "extract_components": true,
            "describe_relationships": true,
            "extract_labels": true
        },
        "embed_config": {
            "auto_embed": true,
            "use_clip": true,
            "priority": "normal"
        }
    }'::jsonb
WHERE name = 'diagram';

-- Image with text (OCR-ready)
UPDATE document_type SET
    requires_file_attachment = TRUE,
    extraction_strategy = 'vision',
    auto_create_note = TRUE,
    note_template = E'# {{filename}}\n\n## Image Description\n{{ai_description}}\n\n## Extracted Text\n{{text_content}}\n\n## Details\n- **Captured:** {{capture_date}}\n- **Dimensions:** {{dimensions}}\n\n---\n*Tags: {{suggested_tags}}*',
    agentic_config = '{
        "generation_prompt": "This image contains text. Extract and describe all visible text, and provide context about what the image shows.",
        "context_requirements": {"needs_vision_model": true, "needs_ocr": true},
        "agent_hints": {
            "extract_text": true,
            "primary_focus": "text_extraction"
        },
        "embed_config": {
            "auto_embed": true,
            "use_clip": true,
            "priority": "high"
        }
    }'::jsonb
WHERE name = 'image-with-text';

-- ============================================================================
-- PART 2: VIDEO DOCTYPES
-- ============================================================================

-- Generic video
UPDATE document_type SET
    requires_file_attachment = TRUE,
    extraction_strategy = 'video_multimodal',
    auto_create_note = TRUE,
    note_template = E'# {{title}}\n\n## Summary\n{{ai_summary}}\n\n## Transcript\n{{transcript}}\n\n## Key Scenes\n{{#each scene_descriptions}}\n- **{{this.timestamp}}**: {{this.description}}\n{{/each}}\n\n## Details\n- **Duration:** {{duration}}\n- **Resolution:** {{resolution}}\n- **Recorded:** {{capture_date}}\n\n---\n*Tags: {{suggested_tags}}*',
    agentic_config = '{
        "generation_prompt": "Describe this video. Summarize the content, identify speakers if any, and highlight key moments.",
        "required_sections": ["Summary"],
        "optional_sections": ["Transcript", "Key Scenes"],
        "context_requirements": {"needs_vision_model": true, "needs_audio_model": true},
        "agent_hints": {
            "extract_keyframes": true,
            "transcribe_audio": true,
            "identify_speakers": true,
            "timestamp_scenes": true
        },
        "embed_config": {
            "auto_embed": true,
            "use_clip": true,
            "priority": "low",
            "chunk_config": {
                "strategy": "semantic",
                "chunk_size": 1000,
                "overlap": 100
            }
        }
    }'::jsonb
WHERE name = 'video';

-- ============================================================================
-- PART 3: AUDIO DOCTYPES
-- ============================================================================

-- Generic audio
UPDATE document_type SET
    requires_file_attachment = TRUE,
    extraction_strategy = 'audio_transcribe',
    auto_create_note = TRUE,
    note_template = E'# {{title}}\n\n## Summary\n{{ai_summary}}\n\n## Transcript\n{{transcript}}\n\n## Topics\n{{#each topics}}\n- {{this}}\n{{/each}}\n\n## Details\n- **Duration:** {{duration}}\n- **Recorded:** {{capture_date}}\n\n---\n*Tags: {{suggested_tags}}*',
    agentic_config = '{
        "generation_prompt": "Transcribe and summarize this audio. Identify speakers if multiple, and highlight key topics discussed.",
        "required_sections": ["Transcript"],
        "optional_sections": ["Summary", "Topics"],
        "context_requirements": {"needs_audio_model": true},
        "agent_hints": {
            "identify_speakers": true,
            "extract_topics": true
        },
        "embed_config": {
            "auto_embed": true,
            "priority": "normal",
            "chunk_config": {
                "strategy": "semantic",
                "chunk_size": 1500,
                "overlap": 150
            }
        }
    }'::jsonb
WHERE name = 'audio';

-- Podcast
UPDATE document_type SET
    requires_file_attachment = TRUE,
    extraction_strategy = 'audio_transcribe',
    auto_create_note = TRUE,
    note_template = E'# {{title}}\n\n## Episode Summary\n{{ai_summary}}\n\n## Transcript\n{{transcript}}\n\n## Topics Discussed\n{{#each topics}}\n- {{this}}\n{{/each}}\n\n## Speakers\n{{#each speakers}}\n- **{{this.name}}**: {{this.speaking_time}}\n{{/each}}\n\n## Show Notes\n{{show_notes}}\n\n## Details\n- **Duration:** {{duration}}\n- **Published:** {{capture_date}}\n\n---\n*Tags: {{suggested_tags}}*',
    agentic_config = '{
        "generation_prompt": "Transcribe and summarize this podcast episode. Identify hosts and guests, extract key topics and takeaways.",
        "required_sections": ["Episode Summary", "Transcript"],
        "optional_sections": ["Topics Discussed", "Speakers", "Show Notes"],
        "context_requirements": {"needs_audio_model": true},
        "agent_hints": {
            "identify_speakers": true,
            "extract_topics": true,
            "generate_show_notes": true,
            "identify_guests": true
        },
        "embed_config": {
            "auto_embed": true,
            "priority": "normal"
        }
    }'::jsonb
WHERE name = 'podcast';

-- ============================================================================
-- PART 4: DOCUMENT DOCTYPES (PDF, etc.)
-- ============================================================================

-- PDF (generic)
UPDATE document_type SET
    requires_file_attachment = TRUE,
    extraction_strategy = 'pdf_text',
    auto_create_note = TRUE,
    note_template = E'# {{title}}\n\n## Summary\n{{ai_summary}}\n\n## Key Points\n{{key_points}}\n\n## Content\n{{extracted_text}}\n\n## Document Info\n- **Author:** {{author}}\n- **Pages:** {{page_count}}\n- **Created:** {{creation_date}}\n\n---\n*Tags: {{suggested_tags}}*'
WHERE name = 'pdf';

-- Academic paper
UPDATE document_type SET
    requires_file_attachment = TRUE,
    extraction_strategy = 'pdf_text',
    auto_create_note = TRUE,
    note_template = E'# {{title}}\n\n## Abstract\n{{abstract}}\n\n## Key Contributions\n{{key_contributions}}\n\n## Methodology\n{{methodology}}\n\n## Results\n{{results}}\n\n## Citation\n{{citation}}\n\n## Full Text\n{{extracted_text}}\n\n## Paper Info\n- **Authors:** {{authors}}\n- **Published:** {{publication_date}}\n- **Venue:** {{venue}}\n- **DOI:** {{doi}}\n\n---\n*Tags: {{suggested_tags}}*',
    agentic_config = '{
        "generation_prompt": "Analyze this academic paper. Extract the abstract, key contributions, methodology, and results. Generate a citation.",
        "required_sections": ["Abstract", "Key Contributions"],
        "optional_sections": ["Methodology", "Results", "Citation"],
        "context_requirements": {"needs_academic_parser": true},
        "agent_hints": {
            "extract_abstract": true,
            "identify_authors": true,
            "extract_citations": true,
            "identify_methodology": true
        },
        "embed_config": {
            "auto_embed": true,
            "truncate_dim": 256,
            "priority": "high"
        }
    }'::jsonb
WHERE name = 'academic-paper';

-- arXiv paper
UPDATE document_type SET
    requires_file_attachment = TRUE,
    extraction_strategy = 'pdf_text',
    auto_create_note = TRUE,
    note_template = E'# {{title}}\n\n## arXiv: {{arxiv_id}}\n\n## Abstract\n{{abstract}}\n\n## Key Contributions\n{{key_contributions}}\n\n## Content\n{{extracted_text}}\n\n## Paper Info\n- **Authors:** {{authors}}\n- **Submitted:** {{submission_date}}\n- **Categories:** {{categories}}\n\n---\n*Tags: {{suggested_tags}}*',
    agentic_config = '{
        "generation_prompt": "Analyze this arXiv paper. Extract the abstract, key contributions, and categorize the research area.",
        "required_sections": ["Abstract"],
        "context_requirements": {"needs_academic_parser": true},
        "agent_hints": {
            "extract_arxiv_id": true,
            "extract_categories": true
        },
        "embed_config": {
            "auto_embed": true,
            "truncate_dim": 256,
            "priority": "high"
        }
    }'::jsonb
WHERE name = 'arxiv';

-- Thesis
UPDATE document_type SET
    requires_file_attachment = TRUE,
    extraction_strategy = 'pdf_text',
    auto_create_note = TRUE,
    agentic_config = '{
        "generation_prompt": "Analyze this thesis or dissertation. Extract the abstract, research questions, methodology, and conclusions.",
        "required_sections": ["Abstract", "Research Questions"],
        "optional_sections": ["Methodology", "Conclusions"],
        "embed_config": {
            "auto_embed": true,
            "truncate_dim": 256,
            "priority": "normal"
        }
    }'::jsonb
WHERE name = 'thesis';

-- Contract
UPDATE document_type SET
    requires_file_attachment = TRUE,
    extraction_strategy = 'pdf_text',
    auto_create_note = TRUE,
    note_template = E'# {{title}}\n\n## Summary\n{{ai_summary}}\n\n## Key Terms\n{{key_terms}}\n\n## Parties\n{{parties}}\n\n## Important Dates\n{{important_dates}}\n\n## Obligations\n{{obligations}}\n\n## Full Text\n{{extracted_text}}\n\n---\n*Tags: {{suggested_tags}}*',
    agentic_config = '{
        "generation_prompt": "Analyze this contract. Identify parties, key terms, obligations, and important dates.",
        "required_sections": ["Summary", "Key Terms", "Parties"],
        "optional_sections": ["Important Dates", "Obligations"],
        "context_requirements": {"needs_legal_parser": true},
        "agent_hints": {
            "extract_parties": true,
            "extract_dates": true,
            "identify_obligations": true,
            "flag_unusual_terms": true
        },
        "embed_config": {
            "auto_embed": true,
            "priority": "high"
        }
    }'::jsonb
WHERE name = 'contract';

-- ============================================================================
-- PART 5: PRESENTATION DOCTYPE
-- ============================================================================

UPDATE document_type SET
    requires_file_attachment = TRUE,
    extraction_strategy = 'pandoc',
    auto_create_note = TRUE,
    note_template = E'# {{title}}\n\n## Overview\n{{ai_summary}}\n\n## Slides\n{{#each slides}}\n### Slide {{this.number}}: {{this.title}}\n{{this.content}}\n{{#if this.notes}}\n*Notes: {{this.notes}}*\n{{/if}}\n\n{{/each}}\n\n## Key Takeaways\n{{key_takeaways}}\n\n---\n*Tags: {{suggested_tags}}*',
    agentic_config = '{
        "generation_prompt": "Analyze this presentation. Summarize each slide and identify key takeaways.",
        "required_sections": ["Overview"],
        "optional_sections": ["Key Takeaways"],
        "agent_hints": {
            "summarize_slides": true,
            "extract_speaker_notes": true
        },
        "embed_config": {
            "auto_embed": true,
            "priority": "normal"
        }
    }'::jsonb
WHERE name = 'presentation';

-- ============================================================================
-- PART 6: ADD PERSONAL-MEMORY DOCTYPE FOR VIDEO MEMORIES
-- ============================================================================

INSERT INTO document_type (
    name, display_name, category, description,
    file_extensions, mime_types,
    chunking_strategy,
    requires_file_attachment, extraction_strategy, auto_create_note,
    note_template,
    agentic_config,
    is_system
) VALUES (
    'personal-memory',
    'Personal Memory',
    'personal',
    'Personal video memories with AI-enhanced descriptions and emotional context',
    ARRAY['.mp4', '.mov', '.avi', '.mkv', '.webm', '.m4v'],
    ARRAY['video/mp4', 'video/quicktime', 'video/x-msvideo', 'video/x-matroska', 'video/webm'],
    'whole',
    TRUE,
    'video_multimodal',
    TRUE,
    E'# {{title}}\n\n## Memory\n{{ai_description}}\n\n## What Happened\n{{ai_summary}}\n\n## Transcript\n{{#each transcript_segments}}\n**{{this.timestamp}} - {{this.speaker}}:** {{this.text}}\n{{/each}}\n\n## Key Moments\n{{#each scene_descriptions}}\n- **{{this.timestamp}}**: {{this.description}}\n{{/each}}\n\n## People & Places\n{{#if people}}- **People:** {{people}}{{/if}}\n{{#if location}}- **Location:** {{location}}{{/if}}\n{{#if event}}- **Event:** {{event}}{{/if}}\n\n## Details\n- **Recorded:** {{capture_date}}\n- **Duration:** {{duration}}\n- **File:** {{filename}} ({{file_size}})\n\n---\n*Tags: {{suggested_tags}}*',
    '{
        "generation_prompt": "Describe this personal video memory with emotional context. Focus on who is present, what activity is happening, and the mood/atmosphere. Generate a nostalgic, personal narrative that captures the essence of this memory.",
        "required_sections": ["Memory", "What Happened"],
        "optional_sections": ["People & Places", "Key Moments", "Transcript"],
        "context_requirements": {
            "needs_vision_model": true,
            "needs_audio_model": true
        },
        "agent_hints": {
            "tone": "personal_nostalgic",
            "extract_people": true,
            "extract_location": true,
            "extract_event_type": true,
            "identify_emotions": true,
            "suggest_related_memories": true,
            "prefer_first_person": false
        },
        "embed_config": {
            "auto_embed": true,
            "use_clip": true,
            "priority": "normal",
            "chunk_config": {
                "strategy": "semantic",
                "chunk_size": 800,
                "overlap": 80
            }
        }
    }'::jsonb,
    TRUE
) ON CONFLICT (name) DO UPDATE SET
    requires_file_attachment = EXCLUDED.requires_file_attachment,
    extraction_strategy = EXCLUDED.extraction_strategy,
    auto_create_note = EXCLUDED.auto_create_note,
    note_template = EXCLUDED.note_template,
    agentic_config = EXCLUDED.agentic_config;

-- ============================================================================
-- PART 7: ADD VOICE-MEMO DOCTYPE FOR QUICK AUDIO RECORDINGS
-- ============================================================================

INSERT INTO document_type (
    name, display_name, category, description,
    file_extensions, mime_types,
    chunking_strategy,
    requires_file_attachment, extraction_strategy, auto_create_note,
    note_template,
    agentic_config,
    is_system
) VALUES (
    'voice-memo',
    'Voice Memo',
    'personal',
    'Quick voice recordings and audio notes',
    ARRAY['.m4a', '.mp3', '.wav', '.ogg', '.webm'],
    ARRAY['audio/mp4', 'audio/mpeg', 'audio/wav', 'audio/ogg', 'audio/webm'],
    'whole',
    TRUE,
    'audio_transcribe',
    TRUE,
    E'# Voice Memo: {{capture_date}}\n\n## Summary\n{{ai_summary}}\n\n## Transcript\n{{transcript}}\n\n## Action Items\n{{#each action_items}}\n- [ ] {{this}}\n{{/each}}\n\n## Details\n- **Duration:** {{duration}}\n- **Recorded:** {{capture_date}}\n\n---\n*Tags: {{suggested_tags}}*',
    '{
        "generation_prompt": "Transcribe this voice memo and extract key points and action items. Summarize the main topics discussed.",
        "required_sections": ["Transcript"],
        "optional_sections": ["Summary", "Action Items"],
        "context_requirements": {
            "needs_audio_model": true
        },
        "agent_hints": {
            "extract_action_items": true,
            "identify_topics": true,
            "format_as_notes": true
        },
        "embed_config": {
            "auto_embed": true,
            "priority": "high"
        }
    }'::jsonb,
    TRUE
) ON CONFLICT (name) DO UPDATE SET
    requires_file_attachment = EXCLUDED.requires_file_attachment,
    extraction_strategy = EXCLUDED.extraction_strategy,
    auto_create_note = EXCLUDED.auto_create_note,
    note_template = EXCLUDED.note_template,
    agentic_config = EXCLUDED.agentic_config;

-- ============================================================================
-- PART 8: ADD SCANNED-DOCUMENT DOCTYPE FOR OCR
-- ============================================================================

INSERT INTO document_type (
    name, display_name, category, description,
    file_extensions, mime_types,
    chunking_strategy,
    requires_file_attachment, extraction_strategy, auto_create_note,
    note_template,
    agentic_config,
    is_system
) VALUES (
    'scanned-document',
    'Scanned Document',
    'docs',
    'Scanned paper documents requiring OCR',
    ARRAY['.pdf', '.png', '.jpg', '.jpeg', '.tiff'],
    ARRAY['application/pdf', 'image/png', 'image/jpeg', 'image/tiff'],
    'per_section',
    TRUE,
    'pdf_ocr',
    TRUE,
    E'# {{title}}\n\n## Summary\n{{ai_summary}}\n\n## Extracted Text\n{{extracted_text}}\n\n## Document Info\n- **Pages:** {{page_count}}\n- **Scanned:** {{scan_date}}\n- **Quality:** {{ocr_confidence}}%\n\n---\n*Tags: {{suggested_tags}}*',
    '{
        "generation_prompt": "Process this scanned document using OCR. Extract all text and summarize the document content.",
        "required_sections": ["Extracted Text"],
        "optional_sections": ["Summary"],
        "context_requirements": {
            "needs_ocr": true
        },
        "agent_hints": {
            "preserve_formatting": true,
            "detect_tables": true,
            "detect_headers": true
        },
        "embed_config": {
            "auto_embed": true,
            "priority": "normal"
        }
    }'::jsonb,
    TRUE
) ON CONFLICT (name) DO UPDATE SET
    requires_file_attachment = EXCLUDED.requires_file_attachment,
    extraction_strategy = EXCLUDED.extraction_strategy,
    auto_create_note = EXCLUDED.auto_create_note,
    note_template = EXCLUDED.note_template,
    agentic_config = EXCLUDED.agentic_config;
