-- Seed: Content-aware revision document types
-- These types describe the *content semantics* of processed media, not the raw file format.
-- The AI revision pipeline uses agentic_config.required_sections to produce type-specific output.
-- Detection uses magic_patterns matching both scene/keyframe descriptions and transcript patterns.

INSERT INTO document_type (name, display_name, category, description, magic_patterns, chunking_strategy, agentic_config, is_system) VALUES

-- Movie / Film: long-form video with narrative structure, characters, scenes
('movie', 'Movie / Film', 'media',
 'Long-form video content with narrative structure, characters, and scenes',
 ARRAY['%### Scene %', '%**Duration**:%', '%**Cast%', '%## Characters%', '%dialog%'],
 'per_section',
 '{
   "generation_prompt": "You are a film analyst. Produce a structured document that captures the narrative arc, characters, and key scenes of this video content. Weave visual scene descriptions with spoken dialog into coherent scene-by-scene coverage.",
   "required_sections": ["Summary", "Synopsis", "Cast & Characters", "Key Scenes", "Themes"]
 }'::jsonb,
 TRUE),

-- Documentary: factual/investigative video with arguments and sources
('documentary', 'Documentary', 'media',
 'Factual or investigative video content with arguments, interviews, and source material',
 ARRAY['%narrator%', '%interview%', '%footage%', '%according to%', '%documentary%'],
 'per_section',
 '{
   "generation_prompt": "You are a documentary analyst. Produce a structured summary that captures the subject matter, key arguments, evidence presented, and notable interview segments.",
   "required_sections": ["Summary", "Subject Overview", "Key Arguments", "Notable Segments", "Sources & References"]
 }'::jsonb,
 TRUE),

-- Short-Form Video: clips, reels, shorts under ~10 minutes
('short-video', 'Short-Form Video', 'media',
 'Short-form video content such as clips, reels, or social media shorts',
 ARRAY['%### Scene %', '%**Duration**:%'],
 'whole',
 '{
   "generation_prompt": "You are a video content editor. Produce a concise summary capturing the key moments and message of this short video.",
   "required_sections": ["Summary", "Key Moments"]
 }'::jsonb,
 TRUE),

-- Educational Video: instructional content with learning objectives
('educational-video', 'Educational Video', 'media',
 'Instructional video content with learning objectives, demonstrations, and step-by-step explanations',
 ARRAY['%step %', '%learn%', '%tutorial%', '%example%', '%demonstrate%', '%how to%'],
 'per_section',
 '{
   "generation_prompt": "You are an educational content analyst. Produce a structured document that captures the learning objectives, key concepts taught, and step-by-step breakdown of the instructional content.",
   "required_sections": ["Summary", "Learning Objectives", "Key Concepts", "Step-by-Step Breakdown"]
 }'::jsonb,
 TRUE),

-- Meeting Recording: multi-speaker discussion with decisions and action items
('meeting-recording', 'Meeting Recording', 'communication',
 'Multi-speaker meeting recording with discussion topics, decisions, and action items',
 ARRAY['%Speaker %', '%agenda%', '%action item%', '%decision%', '%meeting%', '%attendee%'],
 'per_section',
 '{
   "generation_prompt": "You are a meeting analyst. Produce a structured summary that captures who attended, what was discussed, what decisions were made, and what action items were assigned.",
   "required_sections": ["Summary", "Attendees", "Decisions", "Action Items", "Discussion Points"]
 }'::jsonb,
 TRUE),

-- Interview: two-party or panel conversation focused on a subject
('interview', 'Interview', 'communication',
 'Interview or Q&A conversation between interviewer and subject',
 ARRAY['%Speaker %', '%question%', '%answer%', '%interview%', '%Q:%', '%A:%'],
 'per_section',
 '{
   "generation_prompt": "You are an interview analyst. Produce a structured summary capturing the participants, key topics covered, and notable quotes or insights.",
   "required_sections": ["Summary", "Participants", "Key Topics", "Notable Quotes"]
 }'::jsonb,
 TRUE),

-- Lecture / Talk: single-speaker academic or professional presentation
('lecture', 'Lecture / Talk', 'research',
 'Academic lecture, conference talk, or professional presentation',
 ARRAY['%slide%', '%lecture%', '%professor%', '%topic%', '%conclusion%', '%### Scene %'],
 'per_section',
 '{
   "generation_prompt": "You are an academic content analyst. Produce a structured summary capturing the key concepts, learning objectives, and references from this lecture or talk.",
   "required_sections": ["Summary", "Key Concepts", "Learning Objectives", "References"]
 }'::jsonb,
 TRUE),

-- Tutorial / How-To: step-by-step instructional content
('tutorial', 'Tutorial / How-To', 'creative',
 'Step-by-step tutorial or how-to guide with prerequisites and instructions',
 ARRAY['%step %', '%install%', '%prerequisite%', '%first%', '%next%', '%finally%', '%how to%'],
 'per_section',
 '{
   "generation_prompt": "You are a technical writer. Produce a structured tutorial document with clear prerequisites, sequential steps, and practical tips.",
   "required_sections": ["Summary", "Prerequisites", "Step-by-Step Guide", "Tips"]
 }'::jsonb,
 TRUE),

-- General Audio Recording: podcast, voice memo, audio content without strong classification
('general-audio', 'General Audio Recording', 'media',
 'General audio content such as podcasts, voice memos, or audio recordings',
 ARRAY['%Speaker %', '%transcript%', '%audio%', '%recording%'],
 'per_section',
 '{
   "generation_prompt": "You are an audio content analyst. Produce a structured summary capturing the key topics discussed and a timeline of the content.",
   "required_sections": ["Summary", "Key Topics", "Timeline"]
 }'::jsonb,
 TRUE)

ON CONFLICT (name) DO UPDATE SET
  display_name = EXCLUDED.display_name,
  category = EXCLUDED.category,
  description = EXCLUDED.description,
  magic_patterns = EXCLUDED.magic_patterns,
  chunking_strategy = EXCLUDED.chunking_strategy,
  agentic_config = EXCLUDED.agentic_config,
  is_system = EXCLUDED.is_system,
  updated_at = NOW();
