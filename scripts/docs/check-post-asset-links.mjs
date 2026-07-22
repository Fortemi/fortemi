#!/usr/bin/env node
import { existsSync, readdirSync, readFileSync, statSync } from 'node:fs';
import { join, relative } from 'node:path';

const ROOT = process.cwd();
const PAGES_DIR = join(ROOT, 'dist/fortemi-docs/pages');
const BAD_ASSET_ATTR = /\b(?:src|srcset|href)=["']\/assets\/(?:blog|images)\//g;

function walk(dir) {
  if (!existsSync(dir)) return [];
  const out = [];
  for (const entry of readdirSync(dir)) {
    const path = join(dir, entry);
    const stat = statSync(path);
    if (stat.isDirectory()) out.push(...walk(path));
    else out.push(path);
  }
  return out;
}

const violations = [];
for (const file of walk(PAGES_DIR)) {
  const name = relative(PAGES_DIR, file);
  if (!name.startsWith('posts--') || !name.endsWith('.html')) continue;
  const html = readFileSync(file, 'utf8');
  for (const match of html.matchAll(BAD_ASSET_ATTR)) {
    const line = html.slice(0, match.index).split('\n').length;
    violations.push(`${relative(ROOT, file)}:${line}: ${match[0]}`);
  }
}

if (violations.length) {
  console.error('Post pages contain root-relative image asset links.');
  console.error('Use https://docs.fortemi.com/server/assets/... for post body images.');
  for (const violation of violations) console.error(`- ${violation}`);
  process.exit(1);
}

console.log('Post asset links are subpath-safe.');
