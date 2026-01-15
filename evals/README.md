# Matric-Memory Model Evaluation Framework

TypeScript-based evaluation framework for assessing LLM and embedding models for matric-memory.

## Features

- **Embedding Evaluation**: Precision@K, Recall@K, MRR, NDCG metrics
- **LLM Evaluation**: Revision quality, title generation, context understanding
- **Latency Analysis**: p50/p95/p99 percentiles, throughput measurement
- **Similarity Metrics**: Cosine similarity, clustering, semantic accuracy
- **Ollama Integration**: Direct integration with local Ollama models
- **CLI Interface**: Simple command-line tools for running evaluations
- **Type Safety**: Full TypeScript coverage with strict type checking

## Prerequisites

- Node.js 18+
- Ollama running locally on port 11434
- At least one Ollama model installed

## Installation

```bash
cd /home/roctinam/dev/matric-memory/evals
npm install
```

## Quick Start

### Check Ollama Status

```bash
npm run dev status
```

### List Available Models

```bash
npm run dev list
```

### Run Evaluations

```bash
# Full evaluation suite
npm run eval

# Embedding models only
npm run eval:embeddings

# LLM models only
npm run eval:llms

# Generate report from existing data
npm run report
```

## CLI Commands

### `eval`

Run full evaluation suite (both embeddings and LLMs).

```bash
npm run dev eval -- [options]

Options:
  -m, --models <models...>  Specific models to evaluate
  -o, --output <dir>        Output directory (default: ./reports)
  -v, --verbose            Verbose output
```

### `eval:embeddings`

Run embedding model evaluations only.

```bash
npm run eval:embeddings -- [options]
```

### `eval:llms`

Run LLM generation evaluations only.

```bash
npm run eval:llms -- [options]
```

### `report`

Generate report from existing evaluation data.

```bash
npm run report -- [options]

Options:
  -i, --input <dir>         Input directory with evaluation data
  -o, --output <dir>        Output directory (default: ./reports)
  -f, --format <format>     Output format: json, markdown, both (default: both)
```

### `list`

List all available Ollama models.

```bash
npm run dev list
```

### `status`

Check Ollama connection and list available models.

```bash
npm run dev status
```

## Project Structure

```
evals/
├── src/
│   ├── index.ts              # CLI entry point
│   ├── models/
│   │   ├── types.ts          # TypeScript type definitions
│   │   └── ollama.ts         # Ollama API client
│   └── metrics/
│       ├── retrieval.ts      # Precision, Recall, MRR, NDCG
│       ├── similarity.ts     # Cosine similarity, clustering
│       └── latency.ts        # Timing and percentiles
├── datasets/                 # Test datasets
├── scenarios/                # Evaluation scenarios
├── reports/                  # Generated reports
├── package.json
├── tsconfig.json
└── jest.config.js
```

## Development

### Run Tests

```bash
# Run all tests
npm test

# Run tests in watch mode
npm run test:watch

# Run tests with coverage
npm run test:coverage
```

### Build

```bash
npm run build
```

## Ollama API Endpoints

The framework uses the following Ollama API endpoints:

### Embeddings

```
POST http://localhost:11434/api/embed
{
  "model": "nomic-embed-text",
  "input": "text to embed"
}
```

### Generation

```
POST http://localhost:11434/api/generate
{
  "model": "qwen2.5:14b",
  "prompt": "...",
  "stream": false
}
```

### List Models

```
GET http://localhost:11434/api/tags
```

## Metrics

### Retrieval Metrics

- **Precision@K**: Fraction of relevant items in top-K results
- **Recall@K**: Fraction of all relevant items found in top-K
- **MRR**: Mean Reciprocal Rank - average of 1/rank of first relevant result
- **NDCG@K**: Normalized Discounted Cumulative Gain - graded relevance at K

### Similarity Metrics

- **Cosine Similarity**: Measure of vector similarity (-1 to 1)
- **Euclidean Distance**: L2 distance between vectors
- **Similarity Accuracy**: Categorical accuracy (high/medium/low)

### Latency Metrics

- **p50**: Median latency
- **p95**: 95th percentile latency
- **p99**: 99th percentile latency
- **Mean**: Average latency
- **Min/Max**: Range of latencies

## Test Coverage

The project maintains 80% test coverage threshold across:

- Branches: 80%
- Functions: 80%
- Lines: 80%
- Statements: 80%

Run `npm run test:coverage` to generate coverage reports.

## License

MIT
