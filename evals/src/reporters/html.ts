/**
 * HTML Report Generator with embedded Vega-Lite charts
 * Generates a standalone HTML file with interactive visualizations
 */

import type { EvaluationReport } from '../models/types.js';
import {
  generateModelComparisonChart,
  generateRadarChart,
  generateLatencyVsQualityChart,
  generateRetrievalMetricsChart,
} from './charts.js';

/**
 * Generate a complete HTML report with embedded charts
 */
export function generateHTMLReport(report: EvaluationReport): string {
  const embeddingResults = Object.values(report.embeddingResults);
  const llmResults = Object.values(report.llmResults);

  // Generate chart specs
  const modelComparisonSpec = generateModelComparisonChart(llmResults);
  const radarSpec = generateRadarChart(llmResults);
  const latencyQualitySpec = generateLatencyVsQualityChart(llmResults);
  const retrievalSpec = generateRetrievalMetricsChart(embeddingResults);

  const html = `<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="UTF-8">
  <meta name="viewport" content="width=device-width, initial-scale=1.0">
  <title>Matric-Memory Model Evaluation Report</title>
  <script src="https://cdn.jsdelivr.net/npm/vega@5"></script>
  <script src="https://cdn.jsdelivr.net/npm/vega-lite@5"></script>
  <script src="https://cdn.jsdelivr.net/npm/vega-embed@6"></script>
  <style>
    :root {
      --bg-primary: #0d1117;
      --bg-secondary: #161b22;
      --bg-tertiary: #21262d;
      --text-primary: #c9d1d9;
      --text-secondary: #8b949e;
      --accent-blue: #58a6ff;
      --accent-green: #3fb950;
      --accent-yellow: #d29922;
      --accent-red: #f85149;
      --border-color: #30363d;
    }

    * {
      box-sizing: border-box;
      margin: 0;
      padding: 0;
    }

    body {
      font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', 'Noto Sans', Helvetica, Arial, sans-serif;
      background: var(--bg-primary);
      color: var(--text-primary);
      line-height: 1.6;
      padding: 2rem;
      max-width: 1400px;
      margin: 0 auto;
    }

    h1 {
      font-size: 2.5rem;
      font-weight: 600;
      margin-bottom: 0.5rem;
      background: linear-gradient(135deg, var(--accent-blue), var(--accent-green));
      -webkit-background-clip: text;
      -webkit-text-fill-color: transparent;
      background-clip: text;
    }

    h2 {
      font-size: 1.5rem;
      font-weight: 600;
      margin: 2rem 0 1rem;
      color: var(--text-primary);
      border-bottom: 1px solid var(--border-color);
      padding-bottom: 0.5rem;
    }

    h3 {
      font-size: 1.2rem;
      font-weight: 500;
      margin: 1.5rem 0 0.75rem;
      color: var(--text-secondary);
    }

    .meta {
      color: var(--text-secondary);
      font-size: 0.9rem;
      margin-bottom: 2rem;
    }

    .summary-grid {
      display: grid;
      grid-template-columns: repeat(auto-fit, minmax(200px, 1fr));
      gap: 1rem;
      margin: 1.5rem 0;
    }

    .summary-card {
      background: var(--bg-secondary);
      border: 1px solid var(--border-color);
      border-radius: 8px;
      padding: 1.25rem;
    }

    .summary-card .label {
      font-size: 0.8rem;
      color: var(--text-secondary);
      text-transform: uppercase;
      letter-spacing: 0.05em;
      margin-bottom: 0.25rem;
    }

    .summary-card .value {
      font-size: 1.5rem;
      font-weight: 600;
      color: var(--accent-blue);
    }

    .summary-card.best .value {
      color: var(--accent-green);
    }

    .chart-container {
      background: var(--bg-secondary);
      border: 1px solid var(--border-color);
      border-radius: 8px;
      padding: 1.5rem;
      margin: 1.5rem 0;
    }

    .chart-title {
      font-size: 1rem;
      font-weight: 500;
      margin-bottom: 1rem;
      color: var(--text-primary);
    }

    .chart {
      width: 100%;
      min-height: 300px;
    }

    .charts-grid {
      display: grid;
      grid-template-columns: repeat(auto-fit, minmax(500px, 1fr));
      gap: 1.5rem;
    }

    table {
      width: 100%;
      border-collapse: collapse;
      margin: 1rem 0;
      font-size: 0.9rem;
    }

    th, td {
      padding: 0.75rem 1rem;
      text-align: left;
      border-bottom: 1px solid var(--border-color);
    }

    th {
      background: var(--bg-tertiary);
      font-weight: 500;
      color: var(--text-secondary);
      text-transform: uppercase;
      font-size: 0.75rem;
      letter-spacing: 0.05em;
    }

    tr:hover {
      background: var(--bg-tertiary);
    }

    .score-high {
      color: var(--accent-green);
      font-weight: 600;
    }

    .score-medium {
      color: var(--accent-yellow);
      font-weight: 500;
    }

    .score-low {
      color: var(--accent-red);
    }

    .badge {
      display: inline-block;
      padding: 0.2rem 0.5rem;
      border-radius: 4px;
      font-size: 0.75rem;
      font-weight: 500;
      text-transform: uppercase;
    }

    .badge-best {
      background: rgba(63, 185, 80, 0.2);
      color: var(--accent-green);
    }

    .badge-recommended {
      background: rgba(88, 166, 255, 0.2);
      color: var(--accent-blue);
    }

    .recommendations {
      background: linear-gradient(135deg, rgba(88, 166, 255, 0.1), rgba(63, 185, 80, 0.1));
      border: 1px solid var(--border-color);
      border-radius: 8px;
      padding: 1.5rem;
      margin: 1.5rem 0;
    }

    .recommendations h3 {
      margin-top: 0;
      color: var(--accent-blue);
    }

    .recommendation-item {
      display: flex;
      justify-content: space-between;
      align-items: center;
      padding: 0.75rem 0;
      border-bottom: 1px solid var(--border-color);
    }

    .recommendation-item:last-child {
      border-bottom: none;
    }

    .recommendation-label {
      color: var(--text-secondary);
    }

    .recommendation-value {
      font-weight: 600;
      color: var(--accent-green);
    }

    .methodology {
      background: var(--bg-secondary);
      border: 1px solid var(--border-color);
      border-radius: 8px;
      padding: 1.5rem;
      margin: 2rem 0;
    }

    .methodology ul {
      padding-left: 1.5rem;
      color: var(--text-secondary);
    }

    .methodology li {
      margin: 0.5rem 0;
    }

    footer {
      margin-top: 3rem;
      padding-top: 1.5rem;
      border-top: 1px solid var(--border-color);
      color: var(--text-secondary);
      font-size: 0.85rem;
      text-align: center;
    }
  </style>
</head>
<body>
  <header>
    <h1>Matric-Memory Model Evaluation</h1>
    <p class="meta">
      Generated: ${new Date(report.meta.timestamp).toLocaleString()} |
      Duration: ${(report.meta.durationMs / 1000).toFixed(1)}s |
      Models Tested: ${report.meta.modelsTested}
    </p>
  </header>

  <section class="recommendations">
    <h3>Recommendations</h3>
    <div class="recommendation-item">
      <span class="recommendation-label">Best Embedding Model</span>
      <span class="recommendation-value">${report.recommendations.bestEmbedding || 'N/A'}</span>
    </div>
    <div class="recommendation-item">
      <span class="recommendation-label">Best LLM (Quality)</span>
      <span class="recommendation-value">${report.recommendations.bestLLMQuality || 'N/A'}</span>
    </div>
    <div class="recommendation-item">
      <span class="recommendation-label">Best LLM (Balanced)</span>
      <span class="recommendation-value">${report.recommendations.bestLLMBalanced || 'N/A'}</span>
    </div>
    <div class="recommendation-item">
      <span class="recommendation-label">Fastest LLM</span>
      <span class="recommendation-value">${report.recommendations.bestLLMSpeed || 'N/A'}</span>
    </div>
  </section>

  <div class="summary-grid">
    <div class="summary-card">
      <div class="label">Embedding Models</div>
      <div class="value">${embeddingResults.length}</div>
    </div>
    <div class="summary-card">
      <div class="label">LLM Models</div>
      <div class="value">${llmResults.length}</div>
    </div>
    <div class="summary-card best">
      <div class="label">Top Embedding Score</div>
      <div class="value">${embeddingResults.length > 0 ? Math.max(...embeddingResults.map(r => r.overallScore)).toFixed(1) : 'N/A'}</div>
    </div>
    <div class="summary-card best">
      <div class="label">Top LLM Title Score</div>
      <div class="value">${llmResults.length > 0 ? Math.max(...llmResults.map(r => r.dimensions.titleQuality)).toFixed(1) : 'N/A'}</div>
    </div>
  </div>

  <h2>Visualizations</h2>
  <div class="charts-grid">
    <div class="chart-container">
      <div class="chart-title">Model Score Comparison</div>
      <div id="chart-comparison" class="chart"></div>
    </div>
    <div class="chart-container">
      <div class="chart-title">Retrieval Metrics by Embedding Model</div>
      <div id="chart-retrieval" class="chart"></div>
    </div>
    <div class="chart-container">
      <div class="chart-title">Latency vs Quality Trade-off</div>
      <div id="chart-latency" class="chart"></div>
    </div>
    <div class="chart-container">
      <div class="chart-title">LLM Dimension Radar</div>
      <div id="chart-radar" class="chart"></div>
    </div>
  </div>

  <h2>Embedding Model Results</h2>
  <table>
    <thead>
      <tr>
        <th>Model</th>
        <th>Score</th>
        <th>P@5</th>
        <th>MRR</th>
        <th>NDCG@10</th>
        <th>Similarity</th>
        <th>Latency (p95)</th>
        <th>Throughput</th>
      </tr>
    </thead>
    <tbody>
      ${embeddingResults
        .sort((a, b) => b.overallScore - a.overallScore)
        .map((r, i) => `
        <tr>
          <td>${r.modelName} ${i === 0 ? '<span class="badge badge-best">BEST</span>' : ''}</td>
          <td class="${r.overallScore >= 85 ? 'score-high' : r.overallScore >= 70 ? 'score-medium' : 'score-low'}">${r.overallScore.toFixed(1)}</td>
          <td>${(r.metrics.retrieval.precisionAt5 * 100).toFixed(1)}%</td>
          <td>${(r.metrics.retrieval.mrr * 100).toFixed(1)}%</td>
          <td>${(r.metrics.retrieval.ndcgAt10 * 100).toFixed(1)}%</td>
          <td>${(r.metrics.similarity.accuracy * 100).toFixed(1)}%</td>
          <td>${r.metrics.latency.p95.toFixed(0)}ms</td>
          <td>${r.metrics.throughput.toFixed(1)}/s</td>
        </tr>
      `).join('')}
    </tbody>
  </table>

  <h2>LLM Model Results</h2>
  <table>
    <thead>
      <tr>
        <th>Model</th>
        <th>Title Score</th>
        <th>Revision</th>
        <th>Context</th>
        <th>Instruction</th>
        <th>Efficiency</th>
        <th>Latency (p95)</th>
      </tr>
    </thead>
    <tbody>
      ${llmResults
        .sort((a, b) => b.dimensions.titleQuality - a.dimensions.titleQuality)
        .map((r, i) => `
        <tr>
          <td>${r.modelName} ${i === 0 ? '<span class="badge badge-best">BEST</span>' : ''}</td>
          <td class="${r.dimensions.titleQuality >= 0.85 ? 'score-high' : r.dimensions.titleQuality >= 0.7 ? 'score-medium' : 'score-low'}">${(r.dimensions.titleQuality * 100).toFixed(1)}%</td>
          <td>${r.dimensions.revisionQuality > 0 ? r.dimensions.revisionQuality.toFixed(1) : '-'}</td>
          <td>${r.dimensions.contextQuality > 0 ? r.dimensions.contextQuality.toFixed(1) : '-'}</td>
          <td>${r.dimensions.instructionFollowing > 0 ? r.dimensions.instructionFollowing.toFixed(1) : '-'}</td>
          <td>${r.dimensions.efficiency > 0 ? r.dimensions.efficiency.toFixed(1) : '-'}</td>
          <td>${r.metrics.latency.p95.toFixed(0)}ms</td>
        </tr>
      `).join('')}
    </tbody>
  </table>

  <div class="methodology">
    <h3>Methodology</h3>
    <h4>Embedding Model Scoring Weights</h4>
    <ul>
      <li>Precision@5: 20%</li>
      <li>Recall@10: 15%</li>
      <li>MRR (Mean Reciprocal Rank): 20%</li>
      <li>NDCG@10: 20%</li>
      <li>Semantic Accuracy: 15%</li>
      <li>Latency: 5%</li>
      <li>Throughput: 5%</li>
    </ul>
    <h4>LLM Evaluation Dimensions</h4>
    <ul>
      <li>Title Generation: Semantic similarity to ideal titles + format compliance</li>
      <li>Revision Quality: Information preservation, structure, no hallucination (pending)</li>
      <li>Context Understanding: Summary accuracy of linked notes (pending)</li>
    </ul>
  </div>

  <footer>
    <p>Matric-Memory Model Evaluation System | Generated by evals framework</p>
  </footer>

  <script>
    // Chart specifications with dark theme
    const darkTheme = {
      config: {
        background: '#161b22',
        title: { color: '#c9d1d9' },
        axis: {
          domainColor: '#30363d',
          gridColor: '#21262d',
          tickColor: '#30363d',
          labelColor: '#8b949e',
          titleColor: '#c9d1d9'
        },
        legend: {
          labelColor: '#c9d1d9',
          titleColor: '#c9d1d9'
        },
        view: { stroke: 'transparent' }
      }
    };

    const comparisonSpec = ${JSON.stringify(modelComparisonSpec)};
    const retrievalSpec = ${JSON.stringify(retrievalSpec)};
    const latencySpec = ${JSON.stringify(latencyQualitySpec)};
    const radarSpec = ${JSON.stringify(radarSpec)};

    // Merge dark theme with specs
    Object.assign(comparisonSpec, darkTheme);
    Object.assign(retrievalSpec, darkTheme);
    Object.assign(latencySpec, darkTheme);
    Object.assign(radarSpec, darkTheme);

    // Render charts
    vegaEmbed('#chart-comparison', comparisonSpec, { actions: false });
    vegaEmbed('#chart-retrieval', retrievalSpec, { actions: false });
    vegaEmbed('#chart-latency', latencySpec, { actions: false });
    vegaEmbed('#chart-radar', radarSpec, { actions: false });
  </script>
</body>
</html>`;

  return html;
}
