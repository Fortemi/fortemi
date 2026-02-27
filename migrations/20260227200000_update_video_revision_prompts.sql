-- Update generation_prompt for video/audio document types to include keyframe-merging instructions.
-- The extraction pipeline captures keyframes at ~10s intervals as "### Scene N" entries.
-- These prompts now instruct the revision model to group consecutive keyframes into
-- coherent scenes based on visual continuity rather than treating each as independent.

UPDATE document_type SET agentic_config = jsonb_set(
  agentic_config,
  '{generation_prompt}',
  '"You are a film analyst. The input may contain raw keyframe descriptions captured at ~10-second intervals — these are NOT true scene boundaries. Group consecutive keyframes with visual continuity into coherent scenes. For each scene, describe the visual progression across frames and weave in dialog as it occurs. Produce a screenplay-adjacent document that captures the narrative arc, characters, and key scenes."'::jsonb
), updated_at = NOW()
WHERE name = 'movie';

UPDATE document_type SET agentic_config = jsonb_set(
  agentic_config,
  '{generation_prompt}',
  '"You are a documentary analyst. The input may contain raw keyframe descriptions captured at ~10-second intervals — these are NOT true scene boundaries. Group consecutive keyframes showing the same location, interview subject, or archival footage into coherent segments. Describe the visual progression and interweave narration and interview dialog. Produce a structured summary capturing the subject matter, key arguments, and evidence presented."'::jsonb
), updated_at = NOW()
WHERE name = 'documentary';

UPDATE document_type SET agentic_config = jsonb_set(
  agentic_config,
  '{generation_prompt}',
  '"You are a video content editor. The input may contain raw keyframe descriptions captured at ~10-second intervals — these are NOT true scene boundaries. Group consecutive keyframes with visual continuity into coherent scenes. Produce a concise summary capturing the key moments and message of this short video."'::jsonb
), updated_at = NOW()
WHERE name = 'short-video';

UPDATE document_type SET agentic_config = jsonb_set(
  agentic_config,
  '{generation_prompt}',
  '"You are an educational content analyst. The input may contain raw keyframe descriptions captured at ~10-second intervals — these are NOT true scene boundaries. Group consecutive keyframes covering the same demonstration, slide, or concept into coherent sections. Describe what is shown alongside what is explained. Produce a structured document capturing learning objectives, key concepts, and step-by-step breakdown."'::jsonb
), updated_at = NOW()
WHERE name = 'educational-video';

UPDATE document_type SET agentic_config = jsonb_set(
  agentic_config,
  '{generation_prompt}',
  '"You are a meeting analyst. The input may contain raw keyframe descriptions captured at ~10-second intervals — these are NOT true scene boundaries. Group consecutive keyframes covering the same topic or presentation segment into coherent sections. Describe what is shown (slides, demonstrations, whiteboard) alongside who is speaking and what they say. Produce a structured summary capturing attendees, decisions, and action items."'::jsonb
), updated_at = NOW()
WHERE name = 'meeting-recording';

UPDATE document_type SET agentic_config = jsonb_set(
  agentic_config,
  '{generation_prompt}',
  '"You are an interview analyst. The input may contain raw keyframe descriptions captured at ~10-second intervals — these are NOT true scene boundaries. Group consecutive keyframes from the same exchange or topic into coherent segments. Weave visual context with the Q&A dialog. Produce a structured summary capturing participants, key topics, and notable quotes."'::jsonb
), updated_at = NOW()
WHERE name = 'interview';

UPDATE document_type SET agentic_config = jsonb_set(
  agentic_config,
  '{generation_prompt}',
  '"You are an academic content analyst. The input may contain raw keyframe descriptions captured at ~10-second intervals — these are NOT true scene boundaries. Group consecutive keyframes covering the same slide, diagram, or topic into coherent sections. Describe what is shown alongside the speaker''s explanation. Produce a structured summary capturing key concepts, learning objectives, and references."'::jsonb
), updated_at = NOW()
WHERE name = 'lecture';

UPDATE document_type SET agentic_config = jsonb_set(
  agentic_config,
  '{generation_prompt}',
  '"You are a technical writer. The input may contain raw keyframe descriptions captured at ~10-second intervals — these are NOT true scene boundaries. Group consecutive keyframes covering the same step or demonstration into coherent sections. Describe what is shown alongside the instructor''s explanation. Produce a structured tutorial with clear prerequisites, sequential steps, and practical tips."'::jsonb
), updated_at = NOW()
WHERE name = 'tutorial';

UPDATE document_type SET agentic_config = jsonb_set(
  agentic_config,
  '{generation_prompt}',
  '"You are an audio content analyst. The input may contain raw keyframe descriptions captured at ~10-second intervals if the source is video — these are NOT true scene boundaries. Group consecutive segments covering the same topic into coherent sections. Produce a structured summary capturing key topics and a timeline of the content."'::jsonb
), updated_at = NOW()
WHERE name = 'general-audio';
