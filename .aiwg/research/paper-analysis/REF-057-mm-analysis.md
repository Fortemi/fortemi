# REF-057: Contriever Unsupervised Training - matric-memory Analysis

**Paper:** Izacard, G., et al. (2022). Unsupervised Dense Information Retrieval with Contrastive Learning. TMLR.

**Analysis Date:** 2026-01-25
**Relevance:** Future Enhancement - Domain adaptation without labels

---

## Implementation Mapping (Proposed)

| Contriever Concept | Proposed matric-memory Implementation | Location |
|--------------------|---------------------------------------|----------|
| Independent Cropping | Data augmentation for fine-tuning | Training pipeline |
| Contrastive loss | InfoNCE objective | Fine-tuning code |
| Unsupervised pre-training | Domain-specific adaptation | Offline training |
| MoCo queue | Memory bank for negatives | Training infrastructure |

**Current Status:** Not implemented
**Priority:** Low (consider if domain-specific issues arise)

---

## Contriever Training Methodology

### The Labeled Data Problem

Supervised dense retrieval (DPR) requires:
- Queries with known relevant passages
- Expensive to create for new domains
- matric-memory has no relevance labels

Contriever solves this with unsupervised training:

```
Traditional DPR Training:
Query: "What is PostgreSQL?"
Positive: "PostgreSQL is a relational database..."
Negative: Random passages

Problem: Need explicit (query, answer) pairs

Contriever Training:
Document: "PostgreSQL is a relational database that supports..."

Crop 1: "PostgreSQL is a relational" (first 50%)
Crop 2: "database that supports..." (last 50%)

Self-supervision: Crop 1 and Crop 2 should be similar
(They come from the same document)

No labels needed!
```

### Independent Cropping

```
┌─────────────────────────────────────────────────────────────┐
│  Original Document                                           │
│  "PostgreSQL is a powerful relational database system.      │
│   It supports advanced features like JSON, full-text        │
│   search, and custom extensions for vector operations."     │
└─────────────────────────────────────────────────────────────┘
                            │
            ┌───────────────┴───────────────┐
            ▼                               ▼
┌─────────────────────────┐   ┌─────────────────────────┐
│  Crop A (Independent)   │   │  Crop B (Independent)   │
│  "PostgreSQL is a       │   │  "full-text search,     │
│   powerful relational   │   │   and custom extensions │
│   database system."     │   │   for vector operations."│
└─────────────────────────┘   └─────────────────────────┘
            │                               │
            ▼                               ▼
┌─────────────────────────┐   ┌─────────────────────────┐
│  Embedding A            │   │  Embedding B            │
│  [0.023, -0.156, ...]   │   │  [0.028, -0.142, ...]   │
└─────────────────────────┘   └─────────────────────────┘
            │                               │
            └───────────────┬───────────────┘
                            ▼
┌─────────────────────────────────────────────────────────────┐
│  Contrastive Loss: Pull A and B together                    │
│  Push A away from embeddings of other documents             │
└─────────────────────────────────────────────────────────────┘
```

---

## Proposed matric-memory Application

### Domain Adaptation Scenario

If matric-memory search performs poorly on specific content types:

```
Problem: Technical notes about Kubernetes don't match well
- User searches: "k8s pod scheduling"
- Relevant note: "Kubernetes scheduler assigns pods to nodes..."
- Current embedding model doesn't connect "k8s" ↔ "Kubernetes"
```

Contriever-style training could adapt:

```
Training Data: All existing matric-memory notes
Augmentation: Independent Cropping from each note
Result: Model learns domain-specific relationships
```

### Training Pipeline (Proposed)

```python
# scripts/contriever_finetune.py

import torch
from transformers import AutoModel, AutoTokenizer
from torch.utils.data import DataLoader

class ContrieverTrainer:
    def __init__(self, model_name: str, notes: List[str]):
        self.model = AutoModel.from_pretrained(model_name)
        self.tokenizer = AutoTokenizer.from_pretrained(model_name)
        self.notes = notes

    def independent_crop(self, text: str) -> Tuple[str, str]:
        """Generate two independent crops from text"""
        words = text.split()
        mid = len(words) // 2

        # Add randomization
        crop1_end = random.randint(mid - 10, mid + 10)
        crop2_start = random.randint(mid - 10, mid + 10)

        crop1 = " ".join(words[:crop1_end])
        crop2 = " ".join(words[crop2_start:])

        return crop1, crop2

    def contrastive_loss(self, anchor: torch.Tensor, positive: torch.Tensor,
                         negatives: torch.Tensor, temperature: float = 0.05):
        """InfoNCE loss for contrastive learning"""
        # Similarity of anchor to positive
        pos_sim = torch.sum(anchor * positive, dim=-1) / temperature

        # Similarity of anchor to negatives
        neg_sim = torch.matmul(anchor, negatives.T) / temperature

        # Combine and compute softmax loss
        logits = torch.cat([pos_sim.unsqueeze(1), neg_sim], dim=1)
        labels = torch.zeros(logits.size(0), dtype=torch.long)

        return torch.nn.functional.cross_entropy(logits, labels)

    def train_epoch(self, dataloader: DataLoader):
        self.model.train()

        for batch in dataloader:
            # Generate crops
            crop1_batch = []
            crop2_batch = []
            for text in batch:
                c1, c2 = self.independent_crop(text)
                crop1_batch.append(c1)
                crop2_batch.append(c2)

            # Embed both crops
            emb1 = self.embed(crop1_batch)
            emb2 = self.embed(crop2_batch)

            # In-batch negatives: other crop2s are negatives for each crop1
            loss = self.contrastive_loss(emb1, emb2, emb2)

            loss.backward()
            self.optimizer.step()
```

### Integration with matric-memory

```rust
// crates/matric-inference/src/models.rs

pub enum EmbeddingModel {
    NomicEmbedText,           // Default
    ContrieverBase,           // For evaluation
    MatricContriever,         // Domain-adapted
}

impl EmbeddingModel {
    pub fn model_name(&self) -> &str {
        match self {
            Self::NomicEmbedText => "nomic-embed-text",
            Self::ContrieverBase => "contriever-base",
            Self::MatricContriever => "matric-contriever",  // Fine-tuned
        }
    }
}

// Configuration to switch models
pub struct InferenceConfig {
    pub embedding_model: EmbeddingModel,
    // ...
}
```

---

## Benefits for matric-memory

### 1. No Labeled Data Required

**Paper Finding:**
> "Contriever outperforms BM25 on 11/15 BEIR datasets despite using no labeled data." (Table 1)

| Model | Training Data | BEIR Avg |
|-------|---------------|----------|
| BM25 | None | 0.428 |
| DPR | NQ (supervised) | 0.298 |
| **Contriever** | **None (unsupervised)** | **0.445** |

**matric-memory Benefit:**
- Can fine-tune on existing notes without annotation
- Improves as more notes are added
- No manual labeling effort

### 2. Cross-Domain Generalization

**Paper Finding:**
> "Unsupervised pre-training provides better out-of-domain generalization than supervised training." (Table 1)

**matric-memory Benefit:**
- Works across diverse note types (technical, personal, meeting notes)
- Doesn't overfit to specific vocabulary
- Handles evolving knowledge base

### 3. Continuous Adaptation

**Paper Finding:**
> "Contriever can be further adapted to specific domains with additional unsupervised training." (Section 5)

**matric-memory Benefit:**
- Re-train monthly on new notes
- Model improves with user's knowledge growth
- No additional labeling required

---

## Comparison: Contriever vs Current Approach

| Aspect | Current (nomic-embed-text) | Contriever Fine-tuned |
|--------|----------------------------|------------------------|
| Training | General web corpus | matric-memory notes |
| Domain fit | Generic | Domain-specific |
| Vocabulary | General | User's terminology |
| Maintenance | Static | Periodic re-training |
| Complexity | Simple (use pre-trained) | Higher (training infra) |

### When to Consider Contriever Adaptation

**Indicators that fine-tuning would help:**

1. **Domain-specific vocabulary** not well handled
   - Abbreviations: "k8s", "pg", "tf"
   - Project-specific terms

2. **Low semantic search recall** on known-relevant notes
   - User expects note X but it's not in top 10

3. **Growing knowledge base** with specialized content
   - Model's general knowledge doesn't cover user's domain

---

## Implementation Considerations

### Training Infrastructure

**Requirements:**
- GPU with 16GB+ VRAM (single A100 ideal)
- 10,000+ notes for meaningful adaptation
- ~4 hours training time for 100K notes

**Options:**
1. **Cloud training** (AWS, GCP)
   - Spin up GPU instance for training
   - Download fine-tuned model

2. **Local training** (if user has GPU)
   - Script to fine-tune locally
   - Save model to Ollama format

3. **Hosted service** (future)
   - matric-memory cloud offering
   - Fine-tuning as a service

### Model Export to Ollama

```bash
# After fine-tuning, convert to Ollama format
python scripts/export_to_ollama.py \
    --model ./matric-contriever \
    --output matric-contriever-v1

# Import to Ollama
ollama create matric-contriever-v1 -f Modelfile
```

### A/B Testing

```rust
pub async fn search_ab_test(
    pool: &PgPool,
    query: &str,
    test_group: ABTestGroup,
) -> Result<SearchResults> {
    let model = match test_group {
        ABTestGroup::Control => EmbeddingModel::NomicEmbedText,
        ABTestGroup::Treatment => EmbeddingModel::MatricContriever,
    };

    let embedding = embed_with_model(&query, model).await?;
    search_with_embedding(pool, &embedding).await
}
```

---

## Cross-References

### Related Papers

| Paper | Relationship to Contriever |
|-------|---------------------------|
| REF-029 (DPR) | Supervised baseline Contriever improves |
| REF-030 (SBERT) | Similar architecture, different training |
| REF-058 (E5) | Alternative with weak supervision |

### Planned Code Locations

| File | Contriever Usage |
|------|------------------|
| `scripts/contriever_finetune.py` | Training script |
| `scripts/export_to_ollama.py` | Model conversion |
| `crates/matric-inference/src/models.rs` | Model selection |
| `docs/fine-tuning.md` | User guide |

---

## Decision Framework

### Should matric-memory Use Contriever Fine-Tuning?

```
┌─────────────────────────────────────────────────────────────┐
│  Is semantic search quality satisfactory?                    │
└─────────────────────────────────────────────────────────────┘
                    │
         Yes ───────┴─────── No
          │                   │
          ▼                   ▼
┌─────────────────┐   ┌─────────────────┐
│  Keep current   │   │  Is domain-     │
│  model          │   │  specific?      │
└─────────────────┘   └─────────────────┘
                              │
                   Yes ───────┴─────── No
                    │                   │
                    ▼                   ▼
          ┌─────────────────┐   ┌─────────────────┐
          │  Consider       │   │  Try E5 or      │
          │  Contriever     │   │  other model    │
          │  fine-tuning    │   │  (REF-058)      │
          └─────────────────┘   └─────────────────┘
```

---

## Critical Insights for Future Implementation

### 1. Independent Cropping is Key

> "Independent Cropping outperforms other augmentation strategies (deletion, replacement)." (Table 3)

**Implication:** Use IC specifically, not generic augmentation.

### 2. MoCo Queue Improves Quality

> "Memory bank of past embeddings provides more diverse negatives than in-batch alone." (Section 3.2)

**Implication:** Implement momentum contrast for best results.

### 3. Fine-Tuning is Additive

> "Starting from Contriever, domain-specific fine-tuning provides further gains." (Section 5)

**Implication:** Can start with base Contriever, then fine-tune on notes.

### 4. Less Data Needed Than Supervised

> "Unsupervised Contriever with 100K documents matches supervised DPR with 60K labeled pairs." (Section 4)

**Implication:** matric-memory's note corpus is sufficient.

---

## Key Quotes Relevant to matric-memory

> "We show that dense retrieval can be trained without relevance labels, using only raw text." (Abstract)
>
> **Relevance:** Enables matric-memory to improve search without user annotation.

> "Independent Cropping creates training pairs from the same document, providing a self-supervised signal." (Section 2)
>
> **Relevance:** matric-memory notes are the training data.

> "Contriever outperforms BM25 on average while matching or exceeding supervised methods." (Section 4)
>
> **Relevance:** Validates unsupervised training as a serious option.

> "Domain adaptation with continued unsupervised training on target corpus improves results further." (Section 5)
>
> **Relevance:** matric-memory can keep improving with more notes.

---

## Summary

REF-057 (Contriever) offers a path to domain-specific embedding improvement without labeled data. By using Independent Cropping on existing matric-memory notes, a fine-tuned model could better understand user-specific terminology and concepts. This is a lower priority enhancement to explore if search quality issues arise with specific content types.

**Implementation Status:** Not implemented
**Priority:** Low (consider if domain issues arise)
**Prerequisites:** Evidence of domain-specific search quality issues
**Estimated Effort:** 4-6 weeks (including training infrastructure)
**Expected Benefit:** 5-10% recall improvement on domain-specific queries

---

## Revision History

| Date | Changes |
|------|---------|
| 2026-01-25 | Initial analysis |
