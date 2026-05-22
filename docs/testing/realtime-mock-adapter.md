# Realtime Mock Adapter

`matric-api` exposes `realtime::adapters::MockAdapter` for deterministic real-time call integration tests. It is compiled only for tests or when the `mock-rtp` feature is enabled.

```rust
use futures::StreamExt;
use matric_api::realtime::{adapters::MockAdapter, codec::normalize_frame_to_pcm16k, CallTransport};

#[tokio::test]
async fn pipeline_accepts_mock_media() {
    let mut adapter = MockAdapter::builder()
        .provider_call_id("test-call-1")
        .sine_wave(440.0, 40)
        .dtmf_sequence(['1', '2', '#'])
        .seed(42)
        .build();

    let frames: Vec<_> = adapter.frames().collect().await;
    let pcm = normalize_frame_to_pcm16k(&frames[0]).unwrap();
    assert!(!pcm.is_empty());
}
```

The builder supports fixture payloads, generated sine frames, arbitrary `Codec` values, seeded deterministic frame data, DTMF lifecycle events, frame drops, dropped-call events, and first-frame codec mismatch injection. Fixtures live in `crates/matric-api/tests/fixtures/realtime/` for tests that need stable payload bytes.
