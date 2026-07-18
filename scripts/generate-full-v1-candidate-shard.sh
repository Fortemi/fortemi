#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
fixture_root="$repo_root/tests/fixtures/shards"
output="${1:-$fixture_root/full-v1-integrated-candidate.shard}"
receipt="${output}.receipt.json"
stage="$(mktemp -d)"
trap 'rm -rf "$stage"' EXIT

first_note_id="018f2d2d-bc00-7cc8-8ad2-f147d6a2e77a"
second_note_id="018f2d2d-bc00-7cc8-8ad2-f147d6a2e778"
first_attachment_id="018f2d2d-bc00-7cc8-8ad2-f147d6a2e77c"
second_attachment_id="018f2d2d-bc00-7cc8-8ad2-f147d6a2e780"
blob_digest="1098b345e8aacd29e640d3bf724368680c1bfd401b5a9105cb2dc924740c27ad"
blob_checksum="blake3:$blob_digest"
blob_bytes="Fortemi full-v1 attachment fixture"
fixture_key_id="fortemi-fixture-1"
fixture_public_key="6kpsY-KcUgq-9VB7Ey7F-ZVHdq6-vnuSQh7qaRRG0iw"
fixture_signature="7kqCbEl-AUivd37tEyMXNphrIe5lJgHbGSpG8-5zn_4ft8WA4Cl8gHObkg4Suy_KRZQLkL1Ga_Ys-oS4AtE4Bg"
signed_manifest_sha256="b22add185f3c6131c1b297a24443c2fdc3e12ca9026768af6c0c36abb6b9339a"

mkdir -p "$stage/blobs"
printf '%s\n' "$blob_bytes" >"$stage/blobs/$blob_digest"
blob_size="$(wc -c <"$stage/blobs/$blob_digest" | tr -d ' ')"

core="$fixture_root/core-v1-v1.1-valid"
jq -c \
  --arg note_id "$first_note_id" \
  --arg attachment_id "$first_attachment_id" \
  --arg checksum "$blob_checksum" \
  --argjson bytes "$blob_size" \
  '
    .id = $note_id
    | .title = "Integrated full-v1 fixture"
    | .original_content = "Current original source"
    | .revised_content = "Current revised source"
    | .attachments[0].attachment.id = $attachment_id
    | .attachments[0].attachment.path = "integrated-fixture.bin"
    | .attachments[0].attachment.checksum = $checksum
    | .attachments[0].attachment.bytes = $bytes
  ' "$core/notes.jsonl" >"$stage/notes.jsonl"
jq -c \
  --arg note_id "$second_note_id" \
  --arg attachment_id "$second_attachment_id" \
  --arg checksum "$blob_checksum" \
  --argjson bytes "$blob_size" \
  '
    .id = $note_id
    | .title = "Integrated graph peer"
    | .original_content = "Graph peer original"
    | .revised_content = "Graph peer revised"
    | .created_at = "2026-07-18T15:00:00Z"
    | .updated_at = "2026-07-18T15:01:00Z"
    | .attachments[0].attachment.id = $attachment_id
    | .attachments[0].attachment.path = "integrated-fixture-copy.bin"
    | .attachments[0].attachment.checksum = $checksum
    | .attachments[0].attachment.bytes = $bytes
  ' "$core/notes.jsonl" >>"$stage/notes.jsonl"

cp "$core/collections.json" "$stage/collections.json"
cp "$core/tags.json" "$stage/tags.json"
cp "$core/templates.json" "$stage/templates.json"
jq -c \
  --arg from_note_id "$first_note_id" \
  --arg to_note_id "$second_note_id" \
  '
    .from_note_id = $from_note_id
    | .to_note_id = $to_note_id
    | .to_url = null
  ' "$core/links.jsonl" >"$stage/links.jsonl"

copy_family() {
  local source_dir="$1"
  shift
  local filename
  for filename in "$@"; do
    cp "$source_dir/$filename" "$stage/$filename"
  done
}

copy_family "$fixture_root/full-v1-revision-candidate" \
  note_originals.jsonl note_original_history.jsonl \
  note_revised_current.jsonl note_revisions.jsonl
copy_family "$fixture_root/full-v1-embedding-candidate" \
  embedding_configs.json embedding_sets.json \
  embedding_set_members.jsonl embeddings.jsonl
copy_family "$fixture_root/full-v1-provenance-candidate" \
  provenance_edges.jsonl provenance_activities.jsonl
copy_family "$fixture_root/full-v1-spatial-provenance-candidate" \
  named_locations.jsonl provenance_locations.jsonl provenance_devices.jsonl
copy_family "$fixture_root/full-v1-unified-provenance-candidate" \
  provenance_records.jsonl
copy_family "$fixture_root/full-v1-skos-candidate" \
  skos_schemes.json skos_concepts.json skos_labels.jsonl skos_notes.jsonl \
  skos_relations.jsonl skos_mapping_relations.jsonl \
  skos_scheme_memberships.jsonl note_skos_tags.jsonl \
  skos_collections.json skos_collection_members.jsonl
copy_family "$fixture_root/full-v1-graph-candidate" \
  graph_sources.json graph_edges.jsonl communities.json \
  community_assignments.jsonl

sed -i "s/018f4c11-9f14-7d33-8a21-1c80f648e001/$first_note_id/g" \
  "$stage"/note_originals.jsonl \
  "$stage"/note_original_history.jsonl \
  "$stage"/note_revised_current.jsonl \
  "$stage"/note_revisions.jsonl \
  "$stage"/provenance_edges.jsonl \
  "$stage"/provenance_activities.jsonl \
  "$stage"/provenance_records.jsonl
sed -i "s/018f4c11-9f14-7d33-8a21-1c80f648f103/$first_note_id/g" \
  "$stage"/embedding_set_members.jsonl \
  "$stage"/embeddings.jsonl
sed -i \
  -e "s/018f7d2d-bc00-7cc8-8ad2-f147d6a2e709/$first_note_id/g" \
  -e "s/018f7d2d-bc00-7cc8-8ad2-f147d6a2e711/$second_note_id/g" \
  "$stage"/note_skos_tags.jsonl \
  "$stage"/graph_edges.jsonl \
  "$stage"/communities.json \
  "$stage"/community_assignments.jsonl

manifest_source="$fixture_root/full-v1-manifest-candidate/manifest.json"
jq \
  '
    .producer.revision = "candidate-contract-revision-16"
    | .created_at = "2026-07-18T18:30:00Z"
    | .counts.notes = 2
    | .checksums = {}
  ' "$manifest_source" >"$stage/manifest.json"

checksums='{}'
while IFS= read -r filename; do
  digest="$(sha256sum "$stage/$filename" | awk '{print $1}')"
  checksums="$(jq -c --arg filename "$filename" --arg digest "$digest" \
    '. + {($filename): $digest}' <<<"$checksums")"
done < <(jq -r '.components[]' "$stage/manifest.json" | while IFS= read -r component; do
  case "$component" in
    notes|note_originals|note_original_history|note_revised_current|note_revisions|\
    provenance_edges|provenance_activities|named_locations|provenance_locations|\
    provenance_devices|provenance_records|skos_labels|skos_notes|skos_relations|\
    skos_mapping_relations|skos_scheme_memberships|note_skos_tags|\
    skos_collection_members|graph_edges|community_assignments|links|\
    embedding_set_members|embeddings)
      printf '%s.jsonl\n' "$component"
      ;;
    *)
      printf '%s.json\n' "$component"
      ;;
  esac
done)
jq --argjson checksums "$checksums" '.checksums = $checksums' \
  "$stage/manifest.json" >"$stage/manifest.next.json"
mv "$stage/manifest.next.json" "$stage/manifest.json"

manifest_sha256="$(sha256sum "$stage/manifest.json" | awk '{print $1}')"
if [[ "$manifest_sha256" != "$signed_manifest_sha256" ]]; then
  printf 'manifest digest changed; fixture signature must be regenerated\n' >&2
  exit 1
fi
jq -n \
  --arg key_id "$fixture_key_id" \
  --arg public_key "$fixture_public_key" \
  --arg manifest_digest "$manifest_sha256" \
  --arg blob_digest "$blob_digest" \
  --arg signature "$fixture_signature" \
  '{
    format_version: "1",
    signer: {
      key_id: $key_id,
      algorithm: "ed25519",
      public_key: $public_key
    },
    manifest_digest: $manifest_digest,
    blob_digests: [$blob_digest],
    signature: $signature
  }' >"$stage/signature.json"
signature_sha256="$(sha256sum "$stage/signature.json" | awk '{print $1}')"

mkdir -p "$(dirname "$output")"
(
  cd "$stage"
  find . -type f -printf '%P\n' | LC_ALL=C sort |
    tar --format=gnu \
      --mtime="UTC 2026-07-18 18:30:00" \
      --owner=0 --group=0 --numeric-owner \
      -cf - -T -
) | gzip -n -9 >"$output"

archive_sha256="$(sha256sum "$output" | awk '{print $1}')"
jq -n \
  --arg archive "$(realpath --relative-to="$repo_root" "$output")" \
  --arg archive_sha256 "$archive_sha256" \
  --arg manifest_sha256 "$manifest_sha256" \
  --arg signature_sha256 "$signature_sha256" \
  --arg signer_key_id "$fixture_key_id" \
  --arg signer_public_key "$fixture_public_key" \
  --arg blob_checksum "$blob_checksum" \
  --argjson blob_bytes "$blob_size" \
  '{
    profile: "full-v1",
    status: "signed-integrated-candidate",
    archive: $archive,
    archiveSha256: $archive_sha256,
    manifestSha256: $manifest_sha256,
    signatureEntry: "signature.json",
    signatureEnvelopeSha256: $signature_sha256,
    signatureAlgorithm: "ed25519",
    signerKeyId: $signer_key_id,
    signerPublicKey: $signer_public_key,
    signerTrust: "deterministic public test fixture; never trust this key in production",
    componentCount: 33,
    componentChecksumCount: 33,
    noteCount: 2,
    attachmentReferenceCount: 2,
    distinctBlobCount: 1,
    blobChecksum: $blob_checksum,
    blobBytes: $blob_bytes,
    semanticAssertions: [
      "the canonical Ed25519 signature authenticates the exact manifest and blob inventory",
      "all component schemas and cross-component relationships validate",
      "original and revised current content match the note projection",
      "revision parent chain and current revision identity are preserved",
      "two attachment references resolve to one byte-identical sidecar"
    ],
    conformanceClaim: false,
    remainingGates: [
      "supported full-v1 export and import routes",
      "cross-repository producer and consumer receipts"
    ]
  }' >"$receipt"

printf 'generated %s\n' "$output"
printf 'archive sha256: %s\n' "$archive_sha256"
printf 'blob checksum: %s\n' "$blob_checksum"
