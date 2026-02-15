# ADR-026: Dynamic Embedding Config API

**Status:** Accepted
**Date:** 2026-02-01
**Deciders:** Architecture team

## Context

Embedding configurations are currently seeded via SQL migrations. Adding a new model requires:
1. Writing a new migration
2. Rebuilding and deploying the application
3. Running migrations in production

This is inflexible for:
- Experimenting with new models
- User-specific model preferences
- Cloud provider integration (OpenAI, Voyage, Cohere)
- Rapid iteration on embedding strategies

Users need the ability to add, modify, and test embedding configurations at runtime.

## Decision

Expose embedding configuration management through REST API endpoints:

**Endpoints:**
```
GET    /api/v1/embedding-configs           # List all configs
GET    /api/v1/embedding-configs/:id       # Get config by ID
GET    /api/v1/embedding-configs/default   # Get default config
POST   /api/v1/embedding-configs           # Create new config
PUT    /api/v1/embedding-configs/:id       # Update config
DELETE /api/v1/embedding-configs/:id       # Delete config (if not in use)
POST   /api/v1/embedding-configs/:id/test  # Test config with sample text
```

**Create Request Schema:**
```json
{
    "name": "voyage-code-2",
    "description": "Voyage AI code embedding model",
    "model": "voyage-code-2",
    "dimension": 1536,
    "chunk_size": 512,
    "chunk_overlap": 50,

    "provider": "voyage",
    "provider_config": {
        "api_key_env": "VOYAGE_API_KEY",
        "base_url": "https://api.voyageai.com/v1"
    },

    "supports_mrl": false,
    "matryoshka_dims": null,
    "content_types": ["code"],

    "hnsw_m": 16,
    "hnsw_ef_construction": 200
}
```

**Provider Support:**
- `ollama`: Local Ollama instance (default)
- `openai`: OpenAI API
- `voyage`: Voyage AI
- `cohere`: Cohere API
- `custom`: Custom HTTP endpoint

**Security:**
- API keys stored as environment variable references, never in database
- Provider config validated before save
- Rate limiting on test endpoint
- Audit log for config changes

## Consequences

### Positive
- (+) Runtime flexibility: Add models without deployment
- (+) Experimentation: Test new models quickly
- (+) Multi-provider: Support cloud and local models
- (+) User customization: Per-tenant model preferences
- (+) Testability: Verify config before use

### Negative
- (-) Security risk: API key management complexity
- (-) Validation burden: Must verify model compatibility
- (-) Cleanup: Orphaned configs if embedding sets deleted
- (-) Consistency: Different configs may produce incompatible embeddings

## Implementation

**Code Location:**
- Models: `crates/matric-core/src/models.rs` (EmbeddingProvider enum)
- Repository: `crates/matric-db/src/embedding_configs.rs`
- API: `crates/matric-api/src/main.rs`
- Inference: `crates/matric-inference/src/providers/`

**Key Changes:**
- Add `provider` and `provider_config` columns to `embedding_config`
- Add `EmbeddingProvider` enum: Ollama, OpenAI, Voyage, Cohere, Custom
- Create provider abstraction in matric-inference
- Add CRUD endpoints for embedding configs
- Add test endpoint to verify model connectivity

**Migration Safety:**
- Existing configs default to `provider = 'ollama'`
- Backward compatible: existing code paths unchanged
- Delete blocked if config referenced by embedding sets

## References

- Related: ADR-025 (Document Type Registry)
- Stakeholder Request: REQ-CODE-001
