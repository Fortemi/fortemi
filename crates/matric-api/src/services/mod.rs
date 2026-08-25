//! Service layer for business logic.

pub mod chat_stream_store;
pub mod chunking_service;
pub mod idempotency_store;
pub mod inference_breaker;
pub mod ingest_cursor_store;
pub mod ingest_token_store;
pub mod quota;
pub mod reconstruction_service;
pub mod search_cache;
pub mod tag_resolver;
pub mod user_secret_dsar;
pub mod user_secret_rotation;
pub mod user_secrets;

pub use chat_stream_store::ChatStreamStore;
pub use chunking_service::ChunkingService;
pub use idempotency_store::{IdempotencyRecord, IdempotencyStore};
pub use inference_breaker::{
    InferenceBreakerConfig, InferenceBreakerScope, InferenceCircuitBreakerRegistry,
};
pub use ingest_cursor_store::IngestCursorStore;
pub use ingest_token_store::IngestTokenStore;
pub use quota::{
    QuotaIdentityDimension, QuotaLimitPolicy, QuotaReservationOutcome, QuotaStoreError,
    QuotaStoreHealth, QuotaStoreHealthStatus, RedisQuotaCoordinator, RedisRequestQuotaGate,
    RequestQuotaDecision, RequestQuotaIdentity, RequestQuotaPolicy,
};
pub use reconstruction_service::ReconstructionService;
pub use search_cache::{SearchCache, SearchCacheKeyInput};
pub use tag_resolver::TagResolver;
pub use user_secret_dsar::{
    erase_user_secrets_for_confirmed_dsar, UserSecretErasureError, UserSecretErasureReceipt,
};
pub use user_secret_rotation::{
    ensure_user_secret_rewrap_job, run_user_secret_rewrap_batch, run_user_secret_rewrap_worker,
    UserSecretRewrapReceipt, UserSecretRewrapStatus, UserSecretRewrapWorkerConfig,
    UserSecretRotationError,
};
pub use user_secrets::{
    normalize_user_secret_name, normalize_user_secret_provider, seal_user_secret,
    unseal_user_secret, user_secret_context, user_secret_mask, validate_user_secret_value,
    SealedUserSecret, UserSecretServiceError,
};
