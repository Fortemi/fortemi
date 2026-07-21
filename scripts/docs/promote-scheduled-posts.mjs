#!/usr/bin/env node
import { existsSync, mkdirSync, readdirSync, readFileSync, renameSync } from 'node:fs';
import { basename, dirname, resolve } from 'node:path';

const ROOT = resolve(new URL('../..', import.meta.url).pathname);
const SCHEDULED_DIR = resolve(ROOT, 'scheduled-docs/posts');
const LIVE_DIR = resolve(ROOT, 'docs/content/posts');
const args = new Set(process.argv.slice(2));
const dryRun = args.has('--dry-run');
const nowArg = process.argv.find((arg) => arg.startsWith('--now='));
const now = nowArg ? new Date(nowArg.slice('--now='.length)) : new Date();

function frontmatter(src) {
  const match = /^---\n([\s\S]*?)\n---\n?/.exec(src);
  if (!match) return {};
  const data = {};
  for (const line of match[1].split('\n')) {
    const m = /^([A-Za-z0-9_-]+):\s*(.*)$/.exec(line);
    if (m) data[m[1]] = m[2].replace(/^["']|["']$/g, '');
  }
  return data;
}

function due(meta) {
  if (!meta.publish_at) return false;
  const at = new Date(meta.publish_at);
  return !Number.isNaN(at.valueOf()) && at <= now;
}

if (!existsSync(SCHEDULED_DIR)) {
  console.log(`[scheduled-docs] no scheduled directory: ${SCHEDULED_DIR}`);
  process.exit(0);
}

const posts = readdirSync(SCHEDULED_DIR)
  .filter((entry) => entry.endsWith('.md'))
  .map((entry) => {
    const path = resolve(SCHEDULED_DIR, entry);
    const meta = frontmatter(readFileSync(path, 'utf8'));
    return { path, slug: meta.slug || basename(entry, '.md'), meta };
  })
  .filter((post) => due(post.meta));

if (posts.length === 0) {
  console.log('[scheduled-docs] no due posts');
  process.exit(0);
}

mkdirSync(LIVE_DIR, { recursive: true });
for (const post of posts) {
  const target = resolve(LIVE_DIR, `${post.slug}.md`);
  console.log(`[scheduled-docs] promote ${post.path} -> ${target}`);
  if (existsSync(target)) throw new Error(`target already exists: ${target}`);
  if (!dryRun) {
    mkdirSync(dirname(target), { recursive: true });
    renameSync(post.path, target);
  }
}

if (dryRun) console.log('[scheduled-docs] dry run complete');
