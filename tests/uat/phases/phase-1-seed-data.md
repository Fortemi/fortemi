# UAT Phase 1: Seed Data Generation

**Purpose**: Create test data for subsequent phases
**Duration**: ~5 minutes
**Prerequisites**: Phase 0 passed
**Cleanup Required**: Yes (Phase 11)
**Tools Tested**: `create_collection`, `bulk_create_notes`, `list_notes`

> **MCP-First Requirement**: Every test in this phase MUST be executed via MCP tool calls. Do NOT use curl, HTTP API calls, or any other method. If an MCP tool fails or is missing for an operation, **file a bug issue** — do not fall back to the API. The MCP tool name and exact parameters are specified for each test.

---

## Test Collections

### SEED-COLL: Create Collections

**MCP Tool**: `create_collection`

Create three test collections:

```javascript
create_collection({ name: "UAT-Research", description: "Research notes for UAT testing" })
create_collection({ name: "UAT-Projects", description: "Project documentation for UAT testing" })
create_collection({ name: "UAT-Personal", description: "Personal notes for UAT testing" })
```

**Store IDs**: `research_collection_id`, `projects_collection_id`, `personal_collection_id`

---

## Seed Notes

Execute `bulk_create_notes` with the following content:

### SEED-ML-001: Neural Networks Introduction

**MCP Tool**: `bulk_create_notes`

```javascript
{
  content: `# Introduction to Neural Networks

Neural networks are computing systems inspired by biological neural networks.
They consist of layers of interconnected nodes (neurons) that process information.

## Key Components
- **Input Layer**: Receives raw data
- **Hidden Layers**: Process and transform data
- **Output Layer**: Produces final predictions

## Activation Functions
Common activation functions include ReLU, sigmoid, and tanh.

## Related Concepts
Deep learning uses neural networks with many hidden layers.
Backpropagation is the primary training algorithm.`,
  tags: ["uat/ml", "uat/ml/neural-networks", "uat/fundamentals"],
  revision_mode: "none",
  metadata: { domain: "machine-learning", difficulty: "beginner" }
}
```

### SEED-ML-002: Deep Learning Architectures

**MCP Tool**: `bulk_create_notes`

```javascript
{
  content: `# Deep Learning Architectures

Deep learning extends neural networks with specialized architectures.

## Convolutional Neural Networks (CNNs)
CNNs excel at image processing using convolutional layers that detect
spatial patterns like edges, textures, and shapes.

## Recurrent Neural Networks (RNNs)
RNNs process sequential data by maintaining hidden state across time steps.
LSTMs and GRUs address the vanishing gradient problem.

## Transformers
Attention-based architecture that revolutionized NLP. Powers models like
BERT, GPT, and Claude. Self-attention enables parallel processing.`,
  tags: ["uat/ml", "uat/ml/deep-learning", "uat/ml/architectures"],
  revision_mode: "none",
  metadata: { domain: "machine-learning", difficulty: "intermediate" }
}
```

### SEED-ML-003: Backpropagation

**MCP Tool**: `bulk_create_notes`

```javascript
{
  content: `# Backpropagation Algorithm

Backpropagation is the cornerstone of neural network training.

## How It Works
1. **Forward Pass**: Input flows through network to produce output
2. **Loss Calculation**: Compare output with expected result
3. **Backward Pass**: Calculate gradients using chain rule
4. **Weight Update**: Adjust weights using gradient descent

## Mathematical Foundation
The chain rule allows us to compute partial derivatives of the loss
with respect to each weight in the network.

∂L/∂w = ∂L/∂a × ∂a/∂z × ∂z/∂w`,
  tags: ["uat/ml", "uat/ml/training", "uat/ml/neural-networks"],
  revision_mode: "none",
  metadata: { domain: "machine-learning", difficulty: "intermediate" }
}
```

### SEED-RUST-001: Ownership

**MCP Tool**: `bulk_create_notes`

```javascript
{
  content: `# Rust Ownership System

Rust's ownership system ensures memory safety without garbage collection.

## Three Rules
1. Each value has exactly one owner
2. When the owner goes out of scope, the value is dropped
3. Values can be borrowed (referenced) but borrowing has rules

## Borrowing Rules
- You can have either ONE mutable reference OR any number of immutable references
- References must always be valid (no dangling pointers)`,
  tags: ["uat/programming", "uat/programming/rust", "uat/memory-safety"],
  revision_mode: "none",
  metadata: { language: "rust", topic: "ownership" }
}
```

### SEED-RUST-002: Error Handling

**MCP Tool**: `bulk_create_notes`

```javascript
{
  content: `# Rust Error Handling

Rust uses Result and Option types for explicit error handling.

## Result<T, E>
Represents either success (Ok(T)) or failure (Err(E)).

## The ? Operator
Propagates errors automatically, reducing boilerplate.

## Option<T>
Represents optional values - Some(T) or None.
Eliminates null pointer exceptions.`,
  tags: ["uat/programming", "uat/programming/rust", "uat/error-handling"],
  revision_mode: "none",
  metadata: { language: "rust", topic: "error-handling" }
}
```

### SEED-I18N-001: Chinese AI

**MCP Tool**: `bulk_create_notes`

```javascript
{
  content: `# 人工智能简介 (Introduction to AI in Chinese)

人工智能（AI）是计算机科学的一个分支，旨在创建能够执行通常需要人类智能的任务的系统。

## 主要领域
- **机器学习**: 从数据中学习模式
- **自然语言处理**: 理解和生成人类语言
- **计算机视觉**: 分析和理解图像

## 深度学习
深度学习使用多层神经网络来学习数据的复杂表示。`,
  tags: ["uat/i18n", "uat/i18n/chinese", "uat/ml"],
  revision_mode: "none",
  metadata: { language: "zh-CN" }
}
```

### SEED-I18N-002: Arabic AI

**MCP Tool**: `bulk_create_notes`

```javascript
{
  content: `# مقدمة في الذكاء الاصطناعي

الذكاء الاصطناعي هو فرع من علوم الحاسوب يهدف إلى إنشاء أنظمة ذكية.

## المجالات الرئيسية
- التعلم الآلي
- معالجة اللغات الطبيعية
- الرؤية الحاسوبية`,
  tags: ["uat/i18n", "uat/i18n/arabic", "uat/ml"],
  revision_mode: "none",
  metadata: { language: "ar", direction: "rtl" }
}
```

### SEED-I18N-003: Diacritics

**MCP Tool**: `bulk_create_notes`

```javascript
{
  content: `# Café Culture and Naïve Résumé Writing

Testing diacritics and accent marks in content.

## Words with Diacritics
- café (French coffee shop)
- naïve (innocent, simple)
- résumé (summary, CV)
- jalapeño (spicy pepper)
- über (German: over, super)
- Zürich (Swiss city)

These words should be findable with or without accents.`,
  tags: ["uat/i18n", "uat/i18n/diacritics", "uat/search-test"],
  revision_mode: "none",
  metadata: { test_type: "accent-folding" }
}
```

### SEED-EDGE-001: Empty Sections

**MCP Tool**: `bulk_create_notes`

```javascript
{
  content: `# Empty Sections Test

## Section with content
This section has content.

## Empty section

## Another section with content
More content here.`,
  tags: ["uat/edge-cases", "uat/formatting"],
  revision_mode: "none"
}
```

### SEED-EDGE-002: Special Characters

**MCP Tool**: `bulk_create_notes`

```javascript
{
  content: `# Special Characters Test

## Code Symbols
\`{}[]()<>|&^%$#@!\`

## Math Symbols
∑ ∏ ∫ √ ∞ ≠ ≤ ≥ ± × ÷

## Currency
$ € £ ¥ ₹ ₿

## Emoji
🚀 🎉 ✅ ❌ 🔥 💡 🐱 🐶`,
  tags: ["uat/edge-cases", "uat/special-chars"],
  revision_mode: "none"
}
```

---

## Verification

**MCP Tool**: `list_notes`

After creating seed data:

```javascript
list_notes({ tags: ["uat"], limit: 100 })
```

**Expected**: 10 notes with `uat/*` tags

---

## Phase Summary

| Test ID | Name | MCP Tool(s) | Status |
|---------|------|-------------|--------|
| SEED-COLL | Create Collections | `create_collection` | |
| SEED-ML-001 | Neural Networks | `bulk_create_notes` | |
| SEED-ML-002 | Deep Learning | `bulk_create_notes` | |
| SEED-ML-003 | Backpropagation | `bulk_create_notes` | |
| SEED-RUST-001 | Ownership | `bulk_create_notes` | |
| SEED-RUST-002 | Error Handling | `bulk_create_notes` | |
| SEED-I18N-001 | Chinese AI | `bulk_create_notes` | |
| SEED-I18N-002 | Arabic AI | `bulk_create_notes` | |
| SEED-I18N-003 | Diacritics | `bulk_create_notes` | |
| SEED-EDGE-001 | Empty Sections | `bulk_create_notes` | |
| SEED-EDGE-002 | Special Characters | `bulk_create_notes` | |

**Phase Result**: [ ] PASS / [ ] FAIL

**Stored IDs**:
- `research_collection_id`:
- `projects_collection_id`:
- `personal_collection_id`:
- `seed_note_ids`: []
