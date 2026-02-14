# Phase 2C: Attachment Processing Pipeline — Results

**Date**: 2026-02-13
**Version**: v2026.2.8
**Result**: 31 tests — 28 PASS, 1 PARTIAL, 2 FAIL (90.3%)

| Test ID | Name | Status | Details |
|---------|------|--------|---------|
| PROC-001 | Auto-detect Python Code | PASS | code_ast strategy, doc_type python detected |
| PROC-002 | Auto-detect PDF | PASS | pdf_text strategy assigned |
| PROC-003 | Auto-detect Markdown | PASS | text_native strategy, markdown detected |
| PROC-004 | Auto-detect JSON Config | PASS | structured_extract strategy |
| PROC-005 | Auto-detect from MIME Only | PASS | JPEG via MIME → vision strategy |
| PROC-006 | Override with Valid Type | PASS | markdown_type_id override accepted |
| PROC-007a | Invalid Type Override | PARTIAL | Strict validation rejects invalid UUID instead of fallback to auto-detect |
| PROC-008 | No Override Uses Detection | PASS | Rust auto-detected as code_ast |
| PROC-009 | Override MIME-based Detection | PASS | YAML override accepted, structured_extract strategy |
| PROC-010 | Text -> TextNative | PASS | extraction_strategy: text_native confirmed |
| PROC-011 | PDF -> PdfText | PASS | extraction_strategy: pdf_text confirmed |
| PROC-012 | Image -> Vision | PASS | extraction_strategy: vision confirmed |
| PROC-013 | Audio -> AudioTranscribe | FAIL | extraction_strategy: text_native instead of audio_transcribe — **#354 filed** |
| PROC-014 | Code -> CodeAst | PASS | Both Python and Rust confirmed code_ast |
| PROC-015 | Multiple Files One Note | PASS | 3 attachments with independent strategies (code_ast, text_native, vision) |
| PROC-016 | Mixed Types Same Note | PASS | Independent strategies: code_ast + vision |
| PROC-017 | Max 10 Attachments | PASS | 10 attachments listed on single note |
| PROC-018 | Multiple Notes Isolation | PASS | 3 notes × 2 files each, no cross-contamination |
| PROC-019 | Same File Diff Notes | PASS | Shared blob_id, distinct attachment IDs |
| PROC-020 | Text Extraction Plain | PASS | extracted_text present with full content, char_count: 1179 |
| PROC-021 | JSON Structure Extract | PASS | 7 top-level keys extracted (database, embedding, features, etc.) |
| PROC-022 | CSV Structure Extract | PASS | 5 columns (id, name, email, created_at, status), 101 rows |
| PROC-023 | Code Structure Extract | PASS | Class DataProcessor, methods __init__/process/_transform, function main |
| PROC-024 | Empty File Extraction | PASS | Graceful handling, empty extracted_text, char_count: 0 |
| PROC-025 | Upload Creates Job | PASS | Extraction job created within 1 second of upload |
| PROC-026 | Job References Attachment | PASS | Job payload correctly references attachment ID |
| PROC-027 | Job Status Lifecycle | PASS | Job completed in 681ms with full lifecycle timestamps |
| PROC-028 | Failed Extraction No Crash | PASS | Binary-as-JPEG handled gracefully, text_native fallback, system healthy |
| PROC-029 | E2E Text Pipeline | PASS | Upload → extract → search all working, score 1.0 on content search |
| PROC-030 | E2E Code Pipeline | PASS | Rust → code_ast, structs ProcessorConfig/DataProcessor, functions new/process/transform |
| PROC-031 | E2E Multi-File Pipeline | PASS | 3 files (pdf_text + code_ast + vision) on one note, all completed |

## Issues Filed
- **#354**: Audio file (MP3, audio/mpeg) gets extraction_strategy text_native instead of audio_transcribe (regression of #279)

## Stored IDs
- proc_note_id: 019c590e-d6f3-7842-94f3-25e0f405e068
- proc_python_attachment_id: 019c590f-1f70-7372-bd2f-1a6387d2a395
- proc_pdf_attachment_id: 019c5911-78ab-7012-a9ae-51f3f239d8dd
- proc_md_note_id: 019c5911-ee77-7f92-b14f-ecc756a73839
- proc_md_attachment_id: 019c5912-1d32-74d0-88e3-a7cd6ac9664c
- proc_json_attachment_id: 019c5912-f0ce-7650-8e85-90a93714a4d8
- proc_mime_note_id: 019c5913-d510-73c1-bb51-78db3c4ebc99
- proc_override_note_id: 019c5914-d41b-7d33-a01c-575cb4ddf63a
- markdown_type_id: 019c58eb-d021-74ac-ab8f-b4b1c2a7a400
- yaml_type_id: 019c58eb-d022-7e1c-9fa4-cbb52d37d400
- multifile_note_id: 019c591d-66aa-74b3-a72b-5eb2741c26e3
- proc_text_extraction_id: 019c5924-33c6-73f2-84f6-259c213d85ca
- proc_job_note_id: 019c5925-dd5b-7ef1-8c60-69d50ece50a0
- proc_job_id: 019c5926-0ba3-7973-be58-14aff179844c
- e2e_text_note_id: 019c5927-20d9-79d2-b55a-7fd0433c4bbb
- e2e_code_note_id: 019c5927-33db-7802-84c3-2679e12da921
- e2e_multi_note_id: 019c5927-784a-7912-a64c-9a2c1a0b8a97
