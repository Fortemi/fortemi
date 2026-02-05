# Attachment Provenance Tracking - Executive Summary

**Date**: 2026-02-01
**Status**: Research Complete
**Full Report**: [attachment-provenance-tracking.md](./attachment-provenance-tracking.md)

## Recommendation

**Adopt database-centric provenance tracking with W3C PROV-O alignment**

## Why This Approach?

1. **Standards-compliant**: Based on W3C PROV-O and Dublin Core metadata standards
2. **Integrates seamlessly**: Extends matric-memory's existing provenance infrastructure
3. **Rich querying**: Enables complex lineage and derivation queries
4. **Scalable**: Proven database patterns with referential integrity
5. **Portable**: Optional sidecar export for external systems

## Key Components

### Core Tables (Phase 1)

1. **attachment** - File metadata with version tracking and content-addressable storage
2. **attachment_upload** - Upload provenance (who, when, where, how)
3. **attachment_processing** - Processing activity history (extraction, conversion, AI analysis)
4. **attachment_note_link** - Derivation relationships between attachments and notes
5. **provenance_edge extension** - Integrate with existing W3C PROV infrastructure

### Optional Tables (Phase 3)

6. **attachment_access_log** - Access audit trail for compliance

## Standards Coverage

### W3C PROV-O (Primary)
- **Entity-Activity-Agent model**: Tracks what happened, who did it, when
- **Core properties**: wasGeneratedBy, used, wasAttributedTo, wasDerivedFrom
- **Qualified relations**: Detailed metadata for complex provenance graphs

### Dublin Core
- Simple metadata for interoperability
- Standard terms: creator, created, modified, source, isVersionOf
- XML export format for external systems

### DataCite
- Scientific data management patterns
- Relation types: IsDerivedFrom, IsSourceOf, IsVersionOf
- Contributor tracking

## Content-Addressable Storage

Use SHA-256 hashing for:
- **Deduplication**: Same file uploaded twice = stored once
- **Integrity**: Verify file hasn't been tampered with
- **Immutability**: Git-like storage architecture

```
storage/
  ab/cd/abcdef123456...sha256.pdf
```

## Example Provenance Queries

### "Where did this attachment come from?"
```sql
SELECT uploader_id, source_location, uploaded_at, upload_method
FROM attachment a
JOIN attachment_upload au ON a.id = au.attachment_id
WHERE a.id = $1;
```

### "What processing has been done to this file?"
```sql
SELECT activity_type, processor_name, started_at, status
FROM attachment_processing
WHERE attachment_id = $1
ORDER BY started_at;
```

### "What notes were created from this attachment?"
```sql
SELECT n.id, n.title, anl.relation_type, anl.derivation_method
FROM attachment_note_link anl
JOIN note n ON anl.note_id = n.id
WHERE anl.attachment_id = $1;
```

## Implementation Phases

### Phase 1: Core Provenance (Required)
- Create schema (5 tables)
- Implement upload tracking
- Track processing activities
- Link attachments to notes

**Estimated effort**: 2-3 weeks

### Phase 2: Enhanced Features (Recommended)
- Version history
- Content-addressable storage
- Processing status tracking
- Error handling

**Estimated effort**: 1-2 weeks

### Phase 3: Advanced Features (Optional)
- Access audit logging
- Dublin Core XML export
- PROV-O JSON-LD export
- Sidecar file generation
- Provenance visualization API

**Estimated effort**: 2-3 weeks

## Security & Compliance

### Access Control
- Inherit from note security model
- Multi-tenant isolation (tenant_id)
- Visibility levels (private, shared, public)

### Audit Trail
- Track all uploads, processing, access
- Support GDPR "right to delete" (soft deletes)
- Export provenance on request

### Privacy
- Optional access logging (disabled by default)
- Client info anonymization after retention period
- User consent for detailed tracking

## Success Metrics

### Technical
- All attachments have provenance metadata
- Provenance queries return in <100ms
- Zero orphaned files (referential integrity)
- Processing activities tracked automatically

### User Experience
- Users can answer "where did this come from?" in UI
- Version history is navigable
- Export to standard formats works

### Compliance
- Complete audit trail for regulated data
- GDPR export includes provenance
- Right-to-delete cascades correctly

## Industry Comparison

| System | Approach | Strengths | Fit for matric-memory |
|--------|----------|-----------|----------------------|
| Fedora Commons | DB + RDF | Standards-compliant | Similar but lighter |
| Apache Atlas | Graph DB | Auto-extraction | Too heavyweight |
| Git LFS | Content-addressable | Efficient storage | Good storage model |
| Lightroom | DB + sidecar | Query + portability | Hybrid inspiration |

## Next Steps

1. Review schema design with team
2. Create migration `20260202500000_attachment_provenance.sql`
3. Implement upload handlers in Rust
4. Add processing tracking hooks
5. Build provenance query APIs
6. Design UI for viewing provenance chains

## References

- W3C PROV-O: https://www.w3.org/TR/prov-o/
- Dublin Core: https://www.dublincore.org/specifications/dublin-core/dcmi-terms/
- DataCite Schema: https://schema.datacite.org/
- Full research report: [attachment-provenance-tracking.md](./attachment-provenance-tracking.md)
