-- Seed General Document Types Migration
-- Issues #404-#410: Business, Communication, Research, Creative, Media, Personal, and Data document types

-- Business & Legal (#404)
INSERT INTO document_type (name, display_name, category, description, file_extensions, filename_patterns, magic_patterns, chunking_strategy, is_system) VALUES
('contract', 'Contract', 'legal', 'Legal contracts and agreements', ARRAY['.pdf', '.docx', '.doc'], ARRAY['%contract%', '%agreement%'], ARRAY['%WHEREAS%', '%AGREEMENT%', '%hereby agree%'], 'per_section', TRUE),
('policy', 'Policy Document', 'legal', 'Company policies and procedures', ARRAY['.pdf', '.docx', '.md'], ARRAY['%policy%', '%procedure%'], ARRAY['%POLICY%', '%effective date%'], 'per_section', TRUE),
('proposal', 'Business Proposal', 'legal', 'Business proposals and RFPs', ARRAY['.pdf', '.docx', '.md'], ARRAY['%proposal%', '%rfp%'], ARRAY['%PROPOSAL%', '%Executive Summary%'], 'per_section', TRUE),
('invoice', 'Invoice', 'legal', 'Invoices and billing documents', ARRAY['.pdf', '.xlsx', '.csv'], ARRAY['%invoice%', '%bill%'], ARRAY['%INVOICE%', '%Amount Due%', '%Bill To%'], 'whole', TRUE),
('report', 'Business Report', 'legal', 'Business and financial reports', ARRAY['.pdf', '.docx', '.xlsx'], ARRAY['%report%', '%quarterly%', '%annual%'], ARRAY['%REPORT%', '%Executive Summary%'], 'per_section', TRUE),
('sow', 'Statement of Work', 'legal', 'Statements of work and project scopes', ARRAY['.pdf', '.docx', '.md'], ARRAY['%sow%', '%statement-of-work%'], ARRAY['%Statement of Work%', '%Scope of Work%'], 'per_section', TRUE),
('nda', 'Non-Disclosure Agreement', 'legal', 'Non-disclosure and confidentiality agreements', ARRAY['.pdf', '.docx'], ARRAY['%nda%', '%confidentiality%'], ARRAY['%Non-Disclosure%', '%Confidential Information%'], 'per_section', TRUE),
('terms-of-service', 'Terms of Service', 'legal', 'Terms of service and user agreements', ARRAY['.pdf', '.html', '.md'], ARRAY['%tos%', '%terms%', '%eula%'], ARRAY['%Terms of Service%', '%Terms and Conditions%'], 'per_section', TRUE);

-- Communication & Collaboration (#405)
INSERT INTO document_type (name, display_name, category, description, file_extensions, filename_patterns, magic_patterns, chunking_strategy, is_system) VALUES
('email', 'Email', 'communication', 'Individual email messages', ARRAY['.eml', '.msg', '.mbox'], ARRAY['%.eml', '%.msg'], ARRAY['%From:%', '%Subject:%', '%To:%'], 'whole', TRUE),
('email-thread', 'Email Thread', 'communication', 'Email conversation threads', ARRAY['.mbox', '.eml'], ARRAY['%thread%'], ARRAY['%Re:%', '%Fwd:%'], 'semantic', TRUE),
('chat-log', 'Chat Log', 'communication', 'Generic chat conversation logs', ARRAY['.txt', '.log', '.json'], ARRAY['%chat%', '%conversation%'], ARRAY['%[%:%]%'], 'semantic', TRUE),
('slack-export', 'Slack Export', 'communication', 'Slack workspace export data', ARRAY['.json'], ARRAY['%slack%'], ARRAY['%"user":%', '%"ts":%', '%"text":%'], 'per_section', TRUE),
('discord-log', 'Discord Log', 'communication', 'Discord server chat logs', ARRAY['.json', '.txt'], ARRAY['%discord%'], ARRAY['%"id":%', '%"content":%', '%"timestamp":%'], 'per_section', TRUE),
('meeting-notes', 'Meeting Notes', 'communication', 'Meeting minutes and notes', ARRAY['.md', '.docx', '.txt'], ARRAY['%meeting%', '%minutes%'], ARRAY['%Attendees%', '%Action Items%', '%Agenda%'], 'per_section', TRUE),
('transcript', 'Transcript', 'communication', 'Meeting or video transcripts', ARRAY['.txt', '.vtt', '.srt', '.json'], ARRAY['%transcript%', '%.vtt', '%.srt'], ARRAY['%WEBVTT%', '%-->%'], 'semantic', TRUE),
('standup', 'Standup Notes', 'communication', 'Daily standup and status updates', ARRAY['.md', '.txt'], ARRAY['%standup%', '%daily%'], ARRAY['%Yesterday%', '%Today%', '%Blockers%'], 'whole', TRUE);

-- Research & Academic (#406)
INSERT INTO document_type (name, display_name, category, description, file_extensions, filename_patterns, magic_patterns, chunking_strategy, is_system) VALUES
('academic-paper', 'Academic Paper', 'research', 'Scholarly articles and research papers', ARRAY['.pdf', '.tex', '.docx'], ARRAY['%paper%', '%article%'], ARRAY['%Abstract%', '%Introduction%', '%References%'], 'per_section', TRUE),
('arxiv', 'arXiv Paper', 'research', 'arXiv preprints and papers', ARRAY['.pdf', '.tex'], ARRAY['%arxiv%'], ARRAY['%arXiv:%'], 'per_section', TRUE),
('patent', 'Patent', 'research', 'Patent applications and grants', ARRAY['.pdf', '.xml'], ARRAY['%patent%'], ARRAY['%PATENT%', '%Claims%', '%Background%'], 'per_section', TRUE),
('thesis', 'Thesis/Dissertation', 'research', 'Theses and dissertations', ARRAY['.pdf', '.tex', '.docx'], ARRAY['%thesis%', '%dissertation%'], ARRAY['%Abstract%', '%Chapter%'], 'per_section', TRUE),
('citation', 'Citation/BibTeX', 'research', 'Citations and bibliographic entries', ARRAY['.bib', '.ris', '.json'], ARRAY['%.bib', '%.ris'], ARRAY['%@article%', '%@book%', '%@inproceedings%'], 'whole', TRUE),
('literature-review', 'Literature Review', 'research', 'Literature reviews and surveys', ARRAY['.pdf', '.docx', '.md'], ARRAY['%review%', '%survey%'], ARRAY['%Literature Review%', '%Survey%'], 'per_section', TRUE),
('research-note', 'Research Note', 'research', 'Research notes and lab notebooks', ARRAY['.md', '.txt', '.ipynb'], ARRAY['%research%', '%lab-note%'], NULL, 'semantic', TRUE),
('whitepaper', 'Whitepaper', 'research', 'Technical whitepapers', ARRAY['.pdf', '.docx', '.md'], ARRAY['%whitepaper%'], ARRAY['%Executive Summary%', '%Technical Overview%'], 'per_section', TRUE);

-- Creative & Marketing (#407)
INSERT INTO document_type (name, display_name, category, description, file_extensions, filename_patterns, magic_patterns, chunking_strategy, is_system) VALUES
('blog-post', 'Blog Post', 'creative', 'Blog posts and articles', ARRAY['.md', '.html', '.docx'], ARRAY['%blog%', '%post%'], ARRAY['%Published%', '%Author:%'], 'semantic', TRUE),
('article', 'Article', 'creative', 'News articles and long-form content', ARRAY['.md', '.html', '.docx'], ARRAY['%article%'], ARRAY['%By %', '%Published%'], 'semantic', TRUE),
('newsletter', 'Newsletter', 'creative', 'Email newsletters and bulletins', ARRAY['.html', '.md', '.txt'], ARRAY['%newsletter%'], ARRAY['%Unsubscribe%', '%View in browser%'], 'semantic', TRUE),
('press-release', 'Press Release', 'creative', 'Press releases and announcements', ARRAY['.pdf', '.docx', '.md'], ARRAY['%press-release%', '%pr-%'], ARRAY['%FOR IMMEDIATE RELEASE%', '%Contact:%'], 'whole', TRUE),
('social-post', 'Social Media Post', 'creative', 'Social media content', ARRAY['.txt', '.md', '.json'], ARRAY['%tweet%', '%post%'], ARRAY['%#%', '%@%'], 'whole', TRUE),
('ad-copy', 'Ad Copy', 'creative', 'Advertising copy and creative', ARRAY['.txt', '.docx', '.md'], ARRAY['%ad%', '%copy%'], NULL, 'whole', TRUE),
('script', 'Script', 'creative', 'Video/audio scripts and screenplays', ARRAY['.txt', '.fountain', '.pdf'], ARRAY['%script%', '%.fountain'], ARRAY['%INT.%', '%EXT.%', '%FADE IN:%'], 'per_section', TRUE),
('book-chapter', 'Book Chapter', 'creative', 'Book chapters and manuscripts', ARRAY['.md', '.docx', '.tex'], ARRAY['%chapter%'], ARRAY['%Chapter %'], 'semantic', TRUE);

-- Media & Multimedia (#408)
INSERT INTO document_type (name, display_name, category, description, file_extensions, filename_patterns, magic_patterns, chunking_strategy, is_system) VALUES
('image', 'Image', 'media', 'Image files and photos', ARRAY['.jpg', '.jpeg', '.png', '.gif', '.webp', '.svg'], NULL, NULL, 'whole', TRUE),
('image-with-text', 'Image with Text', 'media', 'Images containing text (OCR-ready)', ARRAY['.jpg', '.jpeg', '.png', '.pdf'], ARRAY['%scan%', '%screenshot%'], NULL, 'whole', TRUE),
('screenshot', 'Screenshot', 'media', 'Screenshots and screen captures', ARRAY['.png', '.jpg'], ARRAY['%screenshot%', '%screen-shot%', '%capture%'], NULL, 'whole', TRUE),
('diagram', 'Diagram', 'media', 'Diagrams and technical illustrations', ARRAY['.svg', '.png', '.drawio', '.excalidraw'], ARRAY['%diagram%', '%.drawio', '%.excalidraw'], NULL, 'whole', TRUE),
('audio', 'Audio', 'media', 'Audio files and recordings', ARRAY['.mp3', '.wav', '.m4a', '.flac', '.ogg'], NULL, NULL, 'whole', TRUE),
('video', 'Video', 'media', 'Video files and recordings', ARRAY['.mp4', '.mov', '.avi', '.mkv', '.webm'], NULL, NULL, 'whole', TRUE),
('podcast', 'Podcast', 'media', 'Podcast episodes and audio shows', ARRAY['.mp3', '.m4a'], ARRAY['%podcast%', '%episode%'], NULL, 'whole', TRUE),
('presentation', 'Presentation', 'media', 'Slide presentations', ARRAY['.pptx', '.ppt', '.key', '.odp', '.pdf'], ARRAY['%slides%', '%presentation%'], NULL, 'per_section', TRUE);

-- Personal & Knowledge Management (#409)
INSERT INTO document_type (name, display_name, category, description, file_extensions, filename_patterns, magic_patterns, chunking_strategy, is_system) VALUES
('daily-note', 'Daily Note', 'personal', 'Daily notes and journals', ARRAY['.md', '.txt'], ARRAY['%daily%', '%2024-%', '%2025-%', '%2026-%'], ARRAY['%# 202%-%-%'], 'semantic', TRUE),
('journal', 'Journal Entry', 'personal', 'Personal journal entries', ARRAY['.md', '.txt'], ARRAY['%journal%'], NULL, 'semantic', TRUE),
('bookmark', 'Bookmark', 'personal', 'Web bookmarks and saved links', ARRAY['.html', '.json', '.md'], ARRAY['%bookmark%', '%link%'], ARRAY['%href=%', '%url:%'], 'whole', TRUE),
('highlight', 'Highlight', 'personal', 'Highlights and excerpts', ARRAY['.md', '.txt', '.json'], ARRAY['%highlight%'], ARRAY['%>%'], 'whole', TRUE),
('annotation', 'Annotation', 'personal', 'Annotations and comments', ARRAY['.md', '.txt', '.json'], ARRAY['%annotation%', '%comment%'], NULL, 'whole', TRUE),
('todo-list', 'Todo List', 'personal', 'Task lists and todos', ARRAY['.md', '.txt'], ARRAY['%todo%', '%tasks%'], ARRAY['%- [ ]%', '%- [x]%'], 'whole', TRUE),
('recipe', 'Recipe', 'personal', 'Cooking recipes', ARRAY['.md', '.txt', '.html'], ARRAY['%recipe%'], ARRAY['%Ingredients%', '%Instructions%'], 'per_section', TRUE),
('reading-list', 'Reading List', 'personal', 'Reading lists and book notes', ARRAY['.md', '.txt'], ARRAY['%reading%', '%books%'], NULL, 'semantic', TRUE);

-- Data & Structured Formats (#410)
INSERT INTO document_type (name, display_name, category, description, file_extensions, filename_patterns, magic_patterns, chunking_strategy, is_system) VALUES
('csv', 'CSV', 'data', 'Comma-separated values data', ARRAY['.csv'], NULL, NULL, 'whole', TRUE),
('excel', 'Excel Spreadsheet', 'data', 'Excel workbooks and spreadsheets', ARRAY['.xlsx', '.xls', '.xlsm'], NULL, NULL, 'per_section', TRUE),
('parquet-schema', 'Parquet Schema', 'data', 'Apache Parquet schema definitions', ARRAY['.parquet', '.schema'], ARRAY['%.parquet', '%schema%'], NULL, 'whole', TRUE),
('avro-schema', 'Avro Schema', 'data', 'Apache Avro schema definitions', ARRAY['.avsc', '.avro'], ARRAY['%.avsc'], ARRAY['%"type":%', '%"namespace":%'], 'whole', TRUE),
('xml-data', 'XML Data', 'data', 'XML data files (non-markup)', ARRAY['.xml'], ARRAY['%data.xml', '%export.xml'], ARRAY['<?xml%'], 'per_section', TRUE),
('ndjson', 'NDJSON', 'data', 'Newline-delimited JSON', ARRAY['.ndjson', '.jsonl'], ARRAY['%.ndjson', '%.jsonl'], NULL, 'per_section', TRUE),
('geojson', 'GeoJSON', 'data', 'Geographic JSON data', ARRAY['.geojson', '.json'], ARRAY['%.geojson'], ARRAY['%"type":"Feature%', '%"coordinates":%'], 'whole', TRUE),
('ical', 'iCalendar', 'data', 'Calendar and event data', ARRAY['.ics', '.ical'], ARRAY['%.ics', '%.ical'], ARRAY['%BEGIN:VCALENDAR%', '%BEGIN:VEVENT%'], 'per_section', TRUE);
