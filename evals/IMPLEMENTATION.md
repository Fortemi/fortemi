# Scoring System and Evaluation Runner Implementation

This document summarizes the implementation of the scoring system and evaluation runner for the matric-memory evaluation framework.

## Implemented Files

### 1. src/scoring/weights.ts

Defines all configurable weight constants based on PLAN.md specifications:

**Embedding Model Weights (Section 1.1):**
- Precision@5: 20%
- Recall@10: 15%
- MRR: 20%
- NDCG@10: 20%
- Semantic Accuracy: 15%
- Latency: 5%
- Throughput: 5%

**LLM Dimension Weights (Section 1.2):**
- Revision Quality: 40%
- Title Quality: 20%
- Context Quality: 20%
- Instruction Following: 10%
- Efficiency: 10%

**Sub-dimension Weights:**
- Revision quality components (preservation, structure, hallucination, context, readability)
- Title quality components (relevance, conciseness, uniqueness, format)
- Context quality components (accuracy, clarity, brevity)
- Instruction following components (compliance, adherence, respect)
- Efficiency components (TTFT, total latency, token efficiency)

All weight sets are automatically validated at module load to ensure they sum to 1.0.

### 2. src/scoring/calculator.ts

Implements score calculation functions per PLAN.md Section 3:

**Main Functions:**
- `calculateEmbeddingScore(metrics)` - Weighted combination of embedding metrics
- `calculateLLMScore(dimensions)` - Weighted combination of LLM dimensions
- `normalizeScore(value, min, max)` - Generic 0-100 normalization
- `normalizeLatencyScore(p95)` - Latency-specific normalization: max(0, 100 - p95/10)
- `normalizeThroughputScore(eps)` - Throughput normalization: min(100, eps × 5)
- `calculateWeightedScore(components, weights)` - Generic weighted scorer

**Features:**
- Type-safe with full TypeScript definitions
- Matches exact formulas from PLAN.md
- Handles edge cases (zero values, perfect scores, clamping)

### 3. src/scoring/calculator.test.ts

Comprehensive test suite with 12 tests covering:
- Normalization edge cases (clamping, zero ranges)
- Latency score formula verification
- Throughput score formula verification
- Embedding score calculation accuracy
- LLM score calculation accuracy
- Perfect score handling
- Zero score handling

All tests pass (21/21 total with runner tests).

### 4. src/runner.ts

Evaluation orchestrator that coordinates model testing:

**Model Discovery & Filtering:**
- `filterEmbeddingModels(models, specific?)` - Identifies embedding models by pattern matching
- `filterLLMModels(models, specific?)` - Identifies generation models (excludes embeddings)
- Supports filtering to specific models if requested

**Configuration:**
- `loadConfig(configPath?)` - Loads configuration from JSON file or uses defaults
- Supports overriding via CLI options
- Merges loaded config with defaults

**Output Management:**
- `createOutputDirectory(baseDir)` - Creates timestamped eval-{timestamp} directories
- Automatically creates `raw/` subdirectory for detailed results
- Returns absolute paths

**Evaluation Orchestration:**
- `runEmbeddingEvaluations(models, config, verbose?)` - Runs all embedding tests
- `runLLMEvaluations(models, config, verbose?)` - Runs all LLM tests
- `runFullEvaluation(config, specificModels?)` - Orchestrates complete evaluation suite

**Results Management:**
- Generates recommendations (best embedding, best LLM quality/balanced/speed)
- Saves summary.json with full report
- Saves raw results to raw/embedding-results.json and raw/llm-results.json
- Reports duration and model counts

**Note:** Actual evaluation logic will be implemented when evaluators are created. Currently returns placeholder results with proper structure.

### 5. src/runner.test.ts

Test suite for runner functionality with 9 tests covering:
- Embedding model pattern detection
- LLM model filtering
- Specific model filtering
- Empty input handling
- Output directory creation
- Timestamped directory naming
- Nested directory structure
- Absolute path resolution

### 6. src/index.ts (Updated)

Updated CLI to use the runner:

**eval command:**
- Loads configuration (file or defaults)
- Discovers available models from Ollama
- Filters by type and specific models if requested
- Runs full evaluation suite
- Displays summary with recommendations
- Saves results to timestamped directory

**eval:embeddings command:**
- Runs only embedding model evaluations
- Filters to embedding models
- Reports count of evaluated models

**eval:llms command:**
- Runs only LLM model evaluations
- Filters to generation models
- Reports count of evaluated models

**Additional options:**
- `-c, --config <path>` - Load configuration from file
- `-o, --output <dir>` - Override output directory
- `-v, --verbose` - Enable verbose logging

## Test Coverage

**Total Tests:** 21 passing
- Scoring calculator: 12 tests
- Runner: 9 tests

**Coverage Areas:**
- Score calculation accuracy
- Weight validation
- Normalization formulas
- Model filtering
- Directory management
- Configuration loading
- Result aggregation

## Usage Examples

### Run full evaluation suite
```bash
npm run eval
```

### Run with verbose output
```bash
npm run eval -- -v
```

### Run specific models
```bash
npm run eval -- -m nomic-embed-text qwen2.5:14b
```

### Run only embedding evaluations
```bash
npm run eval:embeddings
```

### Run only LLM evaluations
```bash
npm run eval:llms
```

### Use custom configuration
```bash
npm run eval -- -c ./my-config.json -o ./my-reports
```

## Output Structure

Each evaluation creates a timestamped directory:

```
reports/eval-2025-01-14T12-30-00/
├── summary.json              # Complete evaluation report
└── raw/
    ├── embedding-results.json
    └── llm-results.json
```

### summary.json Structure

```json
{
  "meta": {
    "timestamp": "2025-01-14T12:30:00Z",
    "durationMs": 180000,
    "modelsTested": 16,
    "scenariosRun": 0
  },
  "embeddingResults": {
    "model-name": {
      "modelName": "...",
      "overallScore": 78.5,
      "metrics": { ... }
    }
  },
  "llmResults": {
    "model-name": {
      "modelName": "...",
      "overallScore": 82.3,
      "dimensions": { ... }
    }
  },
  "recommendations": {
    "bestEmbedding": "model-name",
    "bestLLMQuality": "model-name",
    "bestLLMBalanced": "model-name",
    "bestLLMSpeed": "model-name"
  }
}
```

## Next Steps

To complete the evaluation system, the following evaluators need to be implemented:

1. **Embedding Evaluator** (src/evaluators/embedding.ts)
   - Run retrieval queries and calculate metrics
   - Measure similarity accuracy
   - Track latency and throughput

2. **Revision Evaluator** (src/evaluators/revision.ts)
   - Generate revisions in different modes
   - Use LLM-as-judge to score quality
   - Calculate dimension scores

3. **Title Evaluator** (src/evaluators/title.ts)
   - Generate titles from content
   - Evaluate relevance, conciseness, uniqueness
   - Check format compliance

4. **Context Evaluator** (src/evaluators/context.ts)
   - Generate summaries of linked notes
   - Evaluate accuracy and clarity

These evaluators will integrate with the runner through the `runEmbeddingEvaluations` and `runLLMEvaluations` functions.

## Verification

Build and test:
```bash
npm run build    # Compiles TypeScript
npm test         # Runs all tests (21 passing)
```

The implementation follows Test-Driven Development (TDD):
1. Tests written first defining expected behavior
2. Implementation created to pass tests
3. All tests verified passing before completion
4. Clean, documented code ready for integration

## Files Created

1. `/home/roctinam/dev/matric-memory/evals/src/scoring/weights.ts` (142 lines)
2. `/home/roctinam/dev/matric-memory/evals/src/scoring/calculator.ts` (136 lines)
3. `/home/roctinam/dev/matric-memory/evals/src/scoring/calculator.test.ts` (133 lines)
4. `/home/roctinam/dev/matric-memory/evals/src/runner.ts` (398 lines)
5. `/home/roctinam/dev/matric-memory/evals/src/runner.test.ts` (105 lines)
6. `/home/roctinam/dev/matric-memory/evals/src/scoring/README.md` (documentation)
7. `/home/roctinam/dev/matric-memory/evals/src/index.ts` (updated, 257 lines)

Total: ~1,171 lines of production code and tests (excluding documentation)
