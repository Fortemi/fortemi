# ColBERT Quick Start Guide for Matric Memory

**Date:** 2026-01-27
**Full Research:** See `colbert-deployment-research.md`

## TL;DR

- **Ollama does NOT support ColBERT** - need alternative serving
- **Best approach: 2-stage re-ranking** (keep pgvector, add ColBERT re-ranking)
- **Quick win: fastembed-rs** with BGE reranker (pure Rust, 1-2 days)
- **Research-grade: Python service** with RAGatouille/PyLate (3-5 days)

## Decision Tree

```
Do you need the absolute best accuracy?
├─ NO → Use fastembed-rs with BGE reranker
│        - Pure Rust
│        - Fast to implement
│        - Good enough for most cases
│
└─ YES → Deploy Python ColBERT service
         - State-of-art accuracy
         - Research-backed
         - More complexity
```

## Option 1: FastEmbed-rs Reranker (RECOMMENDED START)

### Why This First?
- Pure Rust (no Python dependency)
- 1-2 days implementation
- 5-15% accuracy improvement expected
- Low risk, high value

### Implementation

**1. Add dependency:**
```toml
[dependencies]
fastembed = "5.8"
```

**2. Add reranking to search endpoint:**
```rust
use fastembed::{TextRerank, RerankerModel, InitOptions};

// Initialize once at startup
let reranker = TextRerank::try_new(
    InitOptions::new(RerankerModel::BgeRerankerBase)
)?;

// In search handler:
pub async fn hybrid_search(
    query: &str,
    top_k: usize,
) -> Result<Vec<SearchResult>> {
    // Stage 1: Initial retrieval (existing code)
    let candidates = self.db
        .hybrid_search(query, top_k * 5)  // Retrieve 5x more
        .await?;

    // Stage 2: Re-rank top candidates
    let documents: Vec<String> = candidates
        .iter()
        .map(|c| c.content.clone())
        .collect();

    let reranked = reranker.rerank(
        query,
        &documents,
        true,  // return_documents
        Some(top_k),  // top_n
    )?;

    // Reorder results based on rerank scores
    let mut final_results = Vec::new();
    for rerank_result in reranked {
        final_results.push(candidates[rerank_result.index].clone());
    }

    Ok(final_results)
}
```

**3. Test and measure:**
```bash
# Run test queries
cargo test --package matric-search -- --nocapture

# Benchmark improvement
# Compare MRR, NDCG before/after
```

### Expected Results
- Latency: +50-150ms (CPU)
- Accuracy: +5-15% improvement
- Memory: +200MB
- Complexity: Low

---

## Option 2: Python ColBERT Service (IF NEEDED)

### Why This?
- True ColBERT late interaction
- State-of-art accuracy (10-20% improvement)
- Flexible model selection
- Research-grade results

### Implementation

**1. Create Python service:**

**File: `colbert-service/service.py`**
```python
from fastapi import FastAPI
from pydantic import BaseModel
from ragatouille import RAGPretrainedModel

app = FastAPI()

# Load model at startup
model = RAGPretrainedModel.from_pretrained(
    "jinaai/jina-colbert-v2",
    n_gpu=0  # Use CPU, or 1 for GPU
)

class RerankRequest(BaseModel):
    query: str
    documents: list[str]
    top_n: int = 10

@app.post("/rerank")
def rerank(req: RerankRequest):
    # RAGatouille rerank mode (no indexing needed)
    results = model.rerank(
        query=req.query,
        documents=req.documents,
        k=req.top_n
    )
    return {
        "results": [
            {
                "index": r["result_index"],
                "score": r["score"],
                "content": r["content"]
            }
            for r in results
        ]
    }

@app.get("/health")
def health():
    return {"status": "ok"}
```

**File: `colbert-service/requirements.txt`**
```
fastapi==0.109.0
uvicorn[standard]==0.27.0
ragatouille==0.0.8
torch==2.1.2
```

**File: `colbert-service/Dockerfile`**
```dockerfile
FROM python:3.11-slim

WORKDIR /app

# Install dependencies
COPY requirements.txt .
RUN pip install --no-cache-dir -r requirements.txt

# Copy service
COPY service.py .

# Download model at build time (optional)
# RUN python -c "from ragatouille import RAGPretrainedModel; RAGPretrainedModel.from_pretrained('jinaai/jina-colbert-v2')"

EXPOSE 8000

CMD ["uvicorn", "service:app", "--host", "0.0.0.0", "--port", "8000"]
```

**2. Add Rust client:**

**File: `crates/matric-colbert/src/lib.rs`**
```rust
use reqwest::Client;
use serde::{Deserialize, Serialize};
use anyhow::Result;

#[derive(Debug, Serialize)]
struct RerankRequest {
    query: String,
    documents: Vec<String>,
    top_n: usize,
}

#[derive(Debug, Deserialize)]
struct RerankResponse {
    results: Vec<RerankResult>,
}

#[derive(Debug, Deserialize)]
struct RerankResult {
    index: usize,
    score: f32,
    content: String,
}

pub struct ColBertClient {
    client: Client,
    base_url: String,
}

impl ColBertClient {
    pub fn new(base_url: String) -> Self {
        Self {
            client: Client::new(),
            base_url,
        }
    }

    pub async fn rerank(
        &self,
        query: &str,
        documents: Vec<String>,
        top_n: usize,
    ) -> Result<Vec<RerankResult>> {
        let url = format!("{}/rerank", self.base_url);

        let request = RerankRequest {
            query: query.to_string(),
            documents,
            top_n,
        };

        let response = self.client
            .post(&url)
            .json(&request)
            .timeout(std::time::Duration::from_secs(30))
            .send()
            .await?;

        let rerank_response: RerankResponse = response.json().await?;
        Ok(rerank_response.results)
    }

    pub async fn health(&self) -> Result<bool> {
        let url = format!("{}/health", self.base_url);
        let response = self.client.get(&url).send().await?;
        Ok(response.status().is_success())
    }
}
```

**3. Update search to use ColBERT:**
```rust
// In matric-search/src/lib.rs
use matric_colbert::ColBertClient;

pub struct HybridSearchEngine {
    db: DatabasePool,
    colbert: Option<ColBertClient>,
}

impl HybridSearchEngine {
    pub async fn search(
        &self,
        query: &str,
        top_k: usize,
    ) -> Result<Vec<SearchResult>> {
        // Stage 1: Initial retrieval
        let candidates = self.db
            .hybrid_search(query, top_k * 5)
            .await?;

        // Stage 2: ColBERT re-ranking (if available)
        if let Some(colbert) = &self.colbert {
            let documents: Vec<String> = candidates
                .iter()
                .map(|c| c.content.clone())
                .collect();

            match colbert.rerank(query, documents, top_k).await {
                Ok(reranked) => {
                    // Reorder based on ColBERT scores
                    return Ok(reranked.iter()
                        .map(|r| candidates[r.index].clone())
                        .collect());
                }
                Err(e) => {
                    // Fallback to candidates if ColBERT fails
                    tracing::warn!("ColBERT rerank failed: {}, using fallback", e);
                }
            }
        }

        // Fallback: return candidates
        Ok(candidates.into_iter().take(top_k).collect())
    }
}
```

**4. Docker Compose:**
```yaml
# Add to docker-compose.yml
services:
  colbert:
    build: ./colbert-service
    ports:
      - "8000:8000"
    environment:
      - TORCH_NUM_THREADS=4
    # For GPU support:
    # deploy:
    #   resources:
    #     reservations:
    #       devices:
    #         - driver: nvidia
    #           count: 1
    #           capabilities: [gpu]
```

**5. Configuration:**
```toml
# Add to config.toml
[colbert]
enabled = true
base_url = "http://localhost:8000"
timeout_seconds = 30
```

---

## Performance Comparison

| Approach | Latency | Accuracy | Complexity | Cost |
|----------|---------|----------|------------|------|
| **Baseline** (pgvector only) | 10-20ms | Good | Low | Low |
| **+ fastembed-rs** | 60-170ms | Better (+5-15%) | Low | Low |
| **+ ColBERT service** | 80-250ms | Best (+10-20%) | Medium | Medium |

---

## Which Should You Choose?

### Choose fastembed-rs if:
- You want quick wins
- Pure Rust is important
- Good enough accuracy is acceptable
- Minimal deployment complexity desired

### Choose Python ColBERT if:
- You need state-of-art accuracy
- Research-backed improvements are priority
- You can manage Python service
- Latency <300ms is acceptable

---

## Testing Plan

**1. Establish baseline:**
```bash
# Measure current search quality
cargo test --package matric-search -- --nocapture

# Manual testing
curl -X POST http://localhost:3000/api/notes/search \
  -H "Content-Type: application/json" \
  -d '{"query": "test query", "limit": 10}'
```

**2. Implement Option 1 (fastembed-rs):**
- Add dependency
- Implement re-ranking
- Run tests
- Measure improvement

**3. Decision point:**
- Is 5-15% improvement sufficient? → Ship it!
- Need more accuracy? → Proceed to Option 2

**4. Implement Option 2 (if needed):**
- Deploy Python service
- Add Rust client
- Integration testing
- Benchmark vs Option 1

---

## Deployment Checklist

### For fastembed-rs:
- [ ] Add fastembed dependency
- [ ] Implement reranking in search
- [ ] Test on sample queries
- [ ] Benchmark accuracy improvement
- [ ] Update API documentation
- [ ] Deploy to production

### For Python ColBERT:
- [ ] Create Python service
- [ ] Dockerfile and docker-compose
- [ ] Create Rust client crate
- [ ] Integration tests
- [ ] Benchmark accuracy vs fastembed
- [ ] Set up monitoring (service health)
- [ ] Document failure fallback behavior
- [ ] Deploy alongside API

---

## Monitoring

**Key Metrics:**
- Re-ranking latency (p50, p95, p99)
- Re-ranking accuracy improvement (MRR, NDCG)
- Fallback rate (if ColBERT service fails)
- Memory usage
- CPU/GPU utilization

**Alerts:**
- ColBERT service unhealthy
- Latency >500ms
- Error rate >1%

---

## Cost Estimate

### Option 1: fastembed-rs
- **Development:** 1-2 days
- **Infrastructure:** +200MB RAM, CPU sufficient
- **Ongoing:** None (pure Rust)
- **Risk:** Low

### Option 2: Python ColBERT
- **Development:** 3-5 days
- **Infrastructure:** +2-4GB RAM, GPU optional
- **Ongoing:** Python dependencies, service monitoring
- **Risk:** Medium

---

## Questions?

See full research report: `colbert-deployment-research.md`

**Key takeaway:** Start with fastembed-rs, upgrade to ColBERT if needed. The 2-stage pattern keeps your existing infrastructure while adding state-of-art re-ranking.
