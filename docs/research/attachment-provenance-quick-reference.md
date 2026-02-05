# Provenance Tracking - Quick Reference

**Date**: 2026-02-01
**Related**: [attachment-provenance-tracking.md](./attachment-provenance-tracking.md)

This document provides quick lookup tables for implementing attachment provenance tracking in matric-memory.

---

## Schema Overview

| Table | Purpose | Key Fields | PROV-O Concept |
|-------|---------|------------|----------------|
| attachment | Core file entity | storage_key, checksum_sha256, version, parent_attachment_id | prov:Entity |
| attachment_upload | Upload metadata | uploader_id, source_location, uploaded_at, upload_method | prov:Activity |
| attachment_processing | Processing history | activity_type, processor_name, output_attachment_id, output_note_id | prov:Activity |
| attachment_note_link | File-to-note derivation | relation_type, derivation_method, processing_id | prov:wasDerivedFrom |
| attachment_access_log | Access audit (optional) | accessor_id, accessed_at, access_type | Audit trail |

---

## Provenance Relations

| Relation | SQL Representation | W3C PROV-O | Dublin Core |
|----------|-------------------|------------|-------------|
| Upload | attachment_upload.attachment_id | prov:wasGeneratedBy | dcterms:creator |
| Processing | attachment_processing.attachment_id | prov:used | - |
| Derived file | attachment_processing.output_attachment_id | prov:wasDerivedFrom | dcterms:source |
| Generated note | attachment_processing.output_note_id | prov:wasGeneratedBy | - |
| Version | attachment.parent_attachment_id | prov:wasRevisionOf | dcterms:isVersionOf |
| Source | attachment_upload.source_location | prov:hadPrimarySource | dcterms:source |

---

## Activity Types

| Activity Type | Description | Typical Processor | Example Output |
|--------------|-------------|-------------------|----------------|
| text_extraction | Extract text from PDF/DOC | pdftotext, Apache Tika | note with text content |
| thumbnail_generation | Create preview image | ImageMagick | thumbnail attachment |
| format_conversion | Convert file format | pandoc, ffmpeg | converted attachment |
| ai_analysis | AI processing | Ollama model | note with analysis |
| ocr | Optical character recognition | Tesseract | note with extracted text |
| embedding_generation | Create vector embeddings | nomic-embed-text | embeddings in database |

---

## Relation Types (attachment_note_link)

| Relation Type | Meaning | Inverse |
|--------------|---------|---------|
| attached_to | File attached to note | note has attachment |
| derived_from | Note derived from file | file generated note |
| generated_from | Note auto-generated from file | file was source for note |
| mentioned_in | File mentioned/referenced in note | note references file |
| source_for | File was source for note content | note sourced from file |

---

## Export Formats

| Format | Standard | Use Case | File Extension |
|--------|----------|----------|---------------|
| PROV-O JSON-LD | W3C PROV-O | Standards compliance, research | .jsonld |
| Dublin Core XML | Dublin Core | Library systems, archives | .xml |
| Custom JSON | - | Application-specific | .json |
| Sidecar file | - | Portable metadata | .prov.json |

---

## Query Patterns

| Question | Query Type | Performance | Use Case |
|----------|-----------|-------------|----------|
| Who uploaded this? | Simple JOIN | <10ms | Attribution |
| What processing was done? | Simple SELECT | <10ms | Audit trail |
| What notes came from this file? | JOIN with filtering | <50ms | Lineage tracking |
| Full provenance chain | Recursive CTE | <100ms | Deep analysis |
| Version history | Recursive CTE | <100ms | Time travel |
| All files from source | JOIN with filtering | <50ms | Batch tracking |

---

## Storage Strategy

| Aspect | Implementation | Benefit |
|--------|---------------|---------|
| Deduplication | SHA-256 content addressing | Save storage, detect duplicates |
| Integrity | Cryptographic hash verification | Detect tampering |
| Versioning | parent_attachment_id chain | Track file evolution |
| Immutability | Soft deletes only | Preserve history |
| Path structure | ab/cd/abcdef...sha256.ext | Efficient filesystem |

---

## Security Model

| Level | Implementation | Table/Column |
|-------|---------------|--------------|
| Ownership | owner_id | attachment.owner_id |
| Multi-tenant | tenant_id | attachment.tenant_id |
| Visibility | Inherit from notes | attachment.visibility |
| Soft delete | deleted_at timestamp | attachment.deleted_at |
| Audit | Access logging | attachment_access_log |

---

## Common SQL Snippets

### Get upload provenance
```sql
SELECT a.*, au.uploader_id, au.source_location, au.uploaded_at
FROM attachment a
JOIN attachment_upload au ON a.id = au.attachment_id
WHERE a.id = $1;
```

### Get processing history
```sql
SELECT activity_type, processor_name, started_at, ended_at, status
FROM attachment_processing
WHERE attachment_id = $1
ORDER BY started_at;
```

### Get derived notes
```sql
SELECT n.id, n.title, anl.relation_type, anl.derivation_method
FROM attachment_note_link anl
JOIN note n ON anl.note_id = n.id
WHERE anl.attachment_id = $1;
```

### Check for duplicates (before upload)
```sql
SELECT id, original_filename, created_at
FROM attachment
WHERE checksum_sha256 = $1 AND deleted_at IS NULL;
```

### Get version chain
```sql
WITH RECURSIVE versions AS (
  SELECT id, version, parent_attachment_id, created_at
  FROM attachment WHERE id = $1
  UNION ALL
  SELECT a.id, a.version, a.parent_attachment_id, a.created_at
  FROM attachment a
  JOIN versions v ON a.id = v.parent_attachment_id
)
SELECT * FROM versions ORDER BY version DESC;
```

---

## Implementation Checklist

### Phase 1: Core Provenance
- [ ] Create schema migration
- [ ] Add attachment table
- [ ] Add attachment_upload table
- [ ] Add attachment_processing table
- [ ] Add attachment_note_link table
- [ ] Extend provenance_edge table
- [ ] Implement upload handler
- [ ] Track processing activities
- [ ] Link attachments to notes

### Phase 2: Enhanced Features
- [ ] Implement version tracking
- [ ] Add content-addressable storage
- [ ] Track processing status
- [ ] Add error handling

### Phase 3: Advanced Features
- [ ] Add attachment_access_log
- [ ] Implement Dublin Core export
- [ ] Implement PROV-O JSON-LD export
- [ ] Add sidecar file generation
- [ ] Build provenance visualization API

---

## Standards References

- W3C PROV-O: https://www.w3.org/TR/prov-o/
- W3C PROV-DM: https://www.w3.org/TR/prov-dm/
- Dublin Core Terms: https://www.dublincore.org/specifications/dublin-core/dcmi-terms/
- DataCite Schema: https://schema.datacite.org/
