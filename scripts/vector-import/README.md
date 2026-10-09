# Vector Import Helpers

`parquet_to_ndjson.py` is a reference converter for external embedding loaders.
Fortemi does not ingest Parquet directly; producers convert Parquet rows to the
streamed NDJSON upload contract and post that stream to:

```http
POST /api/v1/embedding-sets/{slug}/runs
Content-Type: application/x-ndjson
```

The converter requires `pyarrow` and expects profile/deletion Parquet columns to
match the upload row fields. Profile rows need `entity_id`, `source`,
`profile_hash`, optional `profile_text`, `template_version`, `space_id`, `dims`,
`embedding`, and optional `metadata`. Deletion rows need `entity_id`, `source`,
and `run_id`.

Example:

```bash
python scripts/vector-import/parquet_to_ndjson.py \
  --run-id run-2026-10-09 \
  --previous-run-id run-2026-10-08 \
  --space-id "$FORTEMI_EMBEDDING_SPACE_ID" \
  --template-version profile-v1 \
  --profiles profiles.parquet \
  --deletions deletions.parquet \
| curl -sS -X POST \
  -H "Authorization: Bearer $FORTEMI_TOKEN" \
  -H "Content-Type: application/x-ndjson" \
  --data-binary @- \
  "$FORTEMI_URL/api/v1/embedding-sets/customer-profiles/runs"
```

`body_sha256` is computed over each emitted non-manifest row line exactly as the
server verifies it: UTF-8 JSON bytes for the row, followed by a single `\n`, in
stream order. The manifest line itself is not included.
