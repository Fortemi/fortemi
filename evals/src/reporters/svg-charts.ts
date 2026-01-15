/**
 * SVG Chart Generator for Markdown Embedding
 * Generates inline SVG charts that can be embedded in markdown reports
 */

import type { EmbeddingEvalResult, LLMEvalResult } from '../models/types.js';

// Chart configuration
const CONFIG = {
  width: 800,
  height: 400,
  padding: { top: 40, right: 120, bottom: 60, left: 200 },
  colors: {
    excellent: '#22c55e',  // Green
    good: '#84cc16',       // Lime
    fair: '#f59e0b',       // Amber
    poor: '#ef4444',       // Red
    primary: '#3b82f6',    // Blue
    secondary: '#8b5cf6',  // Purple
    tertiary: '#06b6d4',   // Cyan
    background: '#f8fafc',
    grid: '#e2e8f0',
    text: '#334155',
    textLight: '#64748b',
  },
  fonts: {
    family: 'system-ui, -apple-system, sans-serif',
    sizeTitle: 16,
    sizeLabel: 12,
    sizeValue: 11,
  },
};

/**
 * Get color based on score value
 */
function getScoreColor(score: number, max: number = 100): string {
  const ratio = score / max;
  if (ratio >= 0.85) return CONFIG.colors.excellent;
  if (ratio >= 0.70) return CONFIG.colors.good;
  if (ratio >= 0.55) return CONFIG.colors.fair;
  return CONFIG.colors.poor;
}

/**
 * Escape text for SVG
 */
function escapeXml(text: string): string {
  return text
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;')
    .replace(/'/g, '&apos;');
}

/**
 * Generate horizontal bar chart for model rankings
 */
export function generateRankingChart(
  data: Array<{ name: string; score: number; badge?: string }>,
  title: string,
  maxScore: number = 100
): string {
  const { width, height, padding, colors, fonts } = CONFIG;
  const chartWidth = width - padding.left - padding.right;
  const chartHeight = height - padding.top - padding.bottom;
  const barHeight = Math.min(30, chartHeight / data.length - 8);
  const barGap = (chartHeight - barHeight * data.length) / (data.length + 1);

  let svg = `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 ${width} ${height}" style="background: ${colors.background}; font-family: ${fonts.family};">`;

  // Title
  svg += `<text x="${width / 2}" y="24" text-anchor="middle" font-size="${fonts.sizeTitle}" font-weight="600" fill="${colors.text}">${escapeXml(title)}</text>`;

  // Grid lines
  for (let i = 0; i <= 4; i++) {
    const x = padding.left + (chartWidth * i) / 4;
    const value = (maxScore * i) / 4;
    svg += `<line x1="${x}" y1="${padding.top}" x2="${x}" y2="${height - padding.bottom}" stroke="${colors.grid}" stroke-dasharray="4,4" />`;
    svg += `<text x="${x}" y="${height - padding.bottom + 20}" text-anchor="middle" font-size="${fonts.sizeValue}" fill="${colors.textLight}">${value.toFixed(0)}</text>`;
  }

  // Bars
  data.forEach((item, index) => {
    const y = padding.top + barGap * (index + 1) + barHeight * index;
    const barWidth = (item.score / maxScore) * chartWidth;
    const color = getScoreColor(item.score, maxScore);

    // Bar
    svg += `<rect x="${padding.left}" y="${y}" width="${barWidth}" height="${barHeight}" fill="${color}" rx="4" />`;

    // Model name
    svg += `<text x="${padding.left - 10}" y="${y + barHeight / 2 + 4}" text-anchor="end" font-size="${fonts.sizeLabel}" fill="${colors.text}">${escapeXml(item.name)}</text>`;

    // Score value
    svg += `<text x="${padding.left + barWidth + 8}" y="${y + barHeight / 2 + 4}" font-size="${fonts.sizeValue}" font-weight="500" fill="${colors.text}">${item.score.toFixed(1)}${item.badge ? ' ' + item.badge : ''}</text>`;
  });

  svg += '</svg>';
  return svg;
}

/**
 * Generate grouped bar chart for metric comparison
 */
export function generateMetricComparisonChart(
  data: Array<{ name: string; metrics: Record<string, number> }>,
  title: string,
  metricColors?: Record<string, string>
): string {
  const { width, height, padding, colors, fonts } = CONFIG;

  if (data.length === 0) return '';

  const metrics = Object.keys(data[0].metrics);
  const chartWidth = width - padding.left - padding.right;
  const chartHeight = height - padding.top - padding.bottom;

  const groupWidth = chartWidth / data.length;
  const barWidth = Math.min(20, (groupWidth - 20) / metrics.length);

  const defaultColors = [CONFIG.colors.primary, CONFIG.colors.secondary, CONFIG.colors.tertiary, CONFIG.colors.excellent, CONFIG.colors.fair];

  let svg = `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 ${width} ${height}" style="background: ${colors.background}; font-family: ${fonts.family};">`;

  // Title
  svg += `<text x="${width / 2}" y="24" text-anchor="middle" font-size="${fonts.sizeTitle}" font-weight="600" fill="${colors.text}">${escapeXml(title)}</text>`;

  // Y-axis grid
  for (let i = 0; i <= 4; i++) {
    const y = height - padding.bottom - (chartHeight * i) / 4;
    const value = (100 * i) / 4;
    svg += `<line x1="${padding.left}" y1="${y}" x2="${width - padding.right}" y2="${y}" stroke="${colors.grid}" stroke-dasharray="4,4" />`;
    svg += `<text x="${padding.left - 10}" y="${y + 4}" text-anchor="end" font-size="${fonts.sizeValue}" fill="${colors.textLight}">${value.toFixed(0)}%</text>`;
  }

  // Bars
  data.forEach((item, groupIndex) => {
    const groupX = padding.left + groupWidth * groupIndex + groupWidth / 2;

    metrics.forEach((metric, metricIndex) => {
      const value = item.metrics[metric];
      const barHeight = (value / 100) * chartHeight;
      const x = groupX - ((metrics.length * barWidth) / 2) + metricIndex * barWidth;
      const y = height - padding.bottom - barHeight;
      const color = metricColors?.[metric] || defaultColors[metricIndex % defaultColors.length];

      svg += `<rect x="${x}" y="${y}" width="${barWidth - 2}" height="${barHeight}" fill="${color}" rx="2" />`;
    });

    // Model name
    svg += `<text x="${groupX}" y="${height - padding.bottom + 20}" text-anchor="middle" font-size="${fonts.sizeValue}" fill="${colors.text}" transform="rotate(-30 ${groupX} ${height - padding.bottom + 20})">${escapeXml(item.name.split(':')[0])}</text>`;
  });

  // Legend
  const legendY = padding.top;
  metrics.forEach((metric, index) => {
    const legendX = width - padding.right + 10;
    const color = metricColors?.[metric] || defaultColors[index % defaultColors.length];
    svg += `<rect x="${legendX}" y="${legendY + index * 18}" width="12" height="12" fill="${color}" rx="2" />`;
    svg += `<text x="${legendX + 18}" y="${legendY + index * 18 + 10}" font-size="${fonts.sizeValue}" fill="${colors.text}">${escapeXml(metric)}</text>`;
  });

  svg += '</svg>';
  return svg;
}

/**
 * Generate scatter plot for latency vs quality tradeoff
 */
export function generateScatterChart(
  data: Array<{ name: string; x: number; y: number; size?: number }>,
  title: string,
  xLabel: string,
  yLabel: string
): string {
  const { width, height, padding, colors, fonts } = CONFIG;
  const chartWidth = width - padding.left - padding.right;
  const chartHeight = height - padding.top - padding.bottom;

  // Calculate ranges
  const xValues = data.map(d => d.x);
  const xMin = 0;
  const xMax = Math.max(...xValues) * 1.1;
  const yMin = 0;
  const yMax = 100;

  let svg = `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 ${width} ${height}" style="background: ${colors.background}; font-family: ${fonts.family};">`;

  // Title
  svg += `<text x="${width / 2}" y="24" text-anchor="middle" font-size="${fonts.sizeTitle}" font-weight="600" fill="${colors.text}">${escapeXml(title)}</text>`;

  // X-axis grid and labels
  for (let i = 0; i <= 4; i++) {
    const x = padding.left + (chartWidth * i) / 4;
    const value = xMin + ((xMax - xMin) * i) / 4;
    svg += `<line x1="${x}" y1="${padding.top}" x2="${x}" y2="${height - padding.bottom}" stroke="${colors.grid}" stroke-dasharray="4,4" />`;
    svg += `<text x="${x}" y="${height - padding.bottom + 20}" text-anchor="middle" font-size="${fonts.sizeValue}" fill="${colors.textLight}">${value >= 1000 ? (value / 1000).toFixed(1) + 's' : value.toFixed(0) + 'ms'}</text>`;
  }

  // Y-axis grid and labels
  for (let i = 0; i <= 4; i++) {
    const y = height - padding.bottom - (chartHeight * i) / 4;
    const value = yMin + ((yMax - yMin) * i) / 4;
    svg += `<line x1="${padding.left}" y1="${y}" x2="${width - padding.right}" y2="${y}" stroke="${colors.grid}" stroke-dasharray="4,4" />`;
    svg += `<text x="${padding.left - 10}" y="${y + 4}" text-anchor="end" font-size="${fonts.sizeValue}" fill="${colors.textLight}">${value.toFixed(0)}%</text>`;
  }

  // Axis labels
  svg += `<text x="${width / 2}" y="${height - 10}" text-anchor="middle" font-size="${fonts.sizeLabel}" fill="${colors.text}">${escapeXml(xLabel)}</text>`;
  svg += `<text x="15" y="${height / 2}" text-anchor="middle" font-size="${fonts.sizeLabel}" fill="${colors.text}" transform="rotate(-90 15 ${height / 2})">${escapeXml(yLabel)}</text>`;

  // Quadrant indicators
  svg += `<text x="${padding.left + 10}" y="${padding.top + 20}" font-size="10" fill="${colors.excellent}" font-weight="500">Fast + High Quality</text>`;
  svg += `<text x="${width - padding.right - 10}" y="${height - padding.bottom - 10}" text-anchor="end" font-size="10" fill="${colors.poor}" font-weight="500">Slow + Low Quality</text>`;

  // Data points
  data.forEach((item) => {
    const x = padding.left + ((item.x - xMin) / (xMax - xMin)) * chartWidth;
    const y = height - padding.bottom - ((item.y - yMin) / (yMax - yMin)) * chartHeight;
    const radius = item.size ? Math.sqrt(item.size) * 2 + 6 : 8;
    const color = getScoreColor(item.y, 100);

    // Point with shadow
    svg += `<circle cx="${x}" cy="${y}" r="${radius + 2}" fill="rgba(0,0,0,0.1)" />`;
    svg += `<circle cx="${x}" cy="${y}" r="${radius}" fill="${color}" stroke="white" stroke-width="2" />`;

    // Label
    const labelOffset = radius + 5;
    svg += `<text x="${x + labelOffset}" y="${y + 4}" font-size="${fonts.sizeValue}" fill="${colors.text}">${escapeXml(item.name.split(':')[0])}</text>`;
  });

  svg += '</svg>';
  return svg;
}

/**
 * Generate summary dashboard with key metrics
 */
export function generateSummaryDashboard(
  bestEmbedding: EmbeddingEvalResult | null,
  bestLLM: LLMEvalResult | null,
  embeddingCount: number,
  llmCount: number
): string {
  const { width, colors, fonts } = CONFIG;
  const dashHeight = 200;
  const cardWidth = (width - 60) / 4;
  const cardHeight = 80;

  let svg = `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 ${width} ${dashHeight}" style="background: ${colors.background}; font-family: ${fonts.family};">`;

  // Title
  svg += `<text x="${width / 2}" y="24" text-anchor="middle" font-size="${fonts.sizeTitle}" font-weight="600" fill="${colors.text}">Evaluation Summary</text>`;

  const cards = [
    { label: 'Embedding Models', value: embeddingCount.toString(), color: CONFIG.colors.primary },
    { label: 'LLM Models', value: llmCount.toString(), color: CONFIG.colors.secondary },
    { label: 'Best Embedding', value: bestEmbedding ? bestEmbedding.overallScore.toFixed(1) : 'N/A', subtext: bestEmbedding?.modelName.split(':')[0] || '', color: CONFIG.colors.excellent },
    { label: 'Best LLM', value: bestLLM ? (bestLLM.dimensions.titleQuality * 100).toFixed(0) + '%' : 'N/A', subtext: bestLLM?.modelName.split(':')[0] || '', color: CONFIG.colors.excellent },
  ];

  cards.forEach((card, index) => {
    const x = 20 + index * (cardWidth + 10);
    const y = 50;

    // Card background
    svg += `<rect x="${x}" y="${y}" width="${cardWidth}" height="${cardHeight}" fill="white" stroke="${colors.grid}" rx="8" />`;

    // Accent bar
    svg += `<rect x="${x}" y="${y}" width="4" height="${cardHeight}" fill="${card.color}" rx="2" />`;

    // Label
    svg += `<text x="${x + 15}" y="${y + 22}" font-size="${fonts.sizeValue}" fill="${colors.textLight}">${escapeXml(card.label)}</text>`;

    // Value
    svg += `<text x="${x + 15}" y="${y + 50}" font-size="20" font-weight="600" fill="${card.color}">${escapeXml(card.value)}</text>`;

    // Subtext
    if (card.subtext) {
      svg += `<text x="${x + 15}" y="${y + 68}" font-size="${fonts.sizeValue}" fill="${colors.textLight}">${escapeXml(card.subtext)}</text>`;
    }
  });

  svg += '</svg>';
  return svg;
}

/**
 * Convert SVG to base64 data URL for markdown embedding
 */
export function svgToDataUrl(svg: string): string {
  const base64 = Buffer.from(svg).toString('base64');
  return `data:image/svg+xml;base64,${base64}`;
}

/**
 * Generate markdown image tag with embedded SVG
 */
export function svgToMarkdownImage(svg: string, alt: string): string {
  const dataUrl = svgToDataUrl(svg);
  return `![${alt}](${dataUrl})`;
}

/**
 * Generate all charts for embedding evaluation results
 */
export function generateEmbeddingCharts(results: EmbeddingEvalResult[]): {
  ranking: string;
  metrics: string;
} {
  // Sort by score
  const sorted = [...results].sort((a, b) => b.overallScore - a.overallScore);

  // Ranking chart
  const rankingData = sorted.map((r, i) => ({
    name: r.modelName,
    score: r.overallScore,
    badge: i === 0 ? '⭐' : undefined,
  }));
  const ranking = generateRankingChart(rankingData, 'Embedding Model Rankings', 100);

  // Metrics comparison
  const metricsData = sorted.map(r => ({
    name: r.modelName,
    metrics: {
      'MRR': r.metrics.retrieval.mrr * 100,
      'P@5': r.metrics.retrieval.precisionAt5 * 100,
      'NDCG': r.metrics.retrieval.ndcgAt10 * 100,
      'Similarity': r.metrics.similarity.accuracy * 100,
    },
  }));
  const metrics = generateMetricComparisonChart(metricsData, 'Retrieval Metrics Comparison');

  return { ranking, metrics };
}

/**
 * Generate all charts for LLM evaluation results
 */
export function generateLLMCharts(results: LLMEvalResult[]): {
  ranking: string;
  scatter: string;
} {
  // Sort by title quality
  const sorted = [...results].sort((a, b) => b.dimensions.titleQuality - a.dimensions.titleQuality);

  // Ranking chart
  const rankingData = sorted.map((r, i) => ({
    name: r.modelName,
    score: r.dimensions.titleQuality * 100,
    badge: i === 0 ? '⭐' : r.dimensions.titleQuality < 0.7 ? '❌' : undefined,
  }));
  const ranking = generateRankingChart(rankingData, 'LLM Title Quality Rankings', 100);

  // Latency vs Quality scatter
  const scatterData = results.map(r => ({
    name: r.modelName,
    x: r.metrics.latency.p95,
    y: r.dimensions.titleQuality * 100,
  }));
  const scatter = generateScatterChart(scatterData, 'Latency vs Quality Tradeoff', 'Latency (P95)', 'Title Quality (%)');

  return { ranking, scatter };
}
