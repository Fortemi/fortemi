#!/usr/bin/env node
import { existsSync, readdirSync, statSync } from 'node:fs';
import { join, relative } from 'node:path';

const ROOT = process.cwd();
const DIST = join(ROOT, 'dist/fortemi-docs');
const SITE = 'https://docs.fortemi.com';
const SERVER = `${SITE}/server`;
const CHUNK_SIZE = 30;

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

const urls = new Set([
  `${SITE}/`,
  `${SITE}/index.html`,
  `${SITE}/sitemap.xml`,
  `${SITE}/robots.txt`,
  `${SITE}/llms.txt`,
  `${SERVER}/`,
  `${SERVER}/index.html`,
  `${SERVER}/sitemap.xml`,
  `${SERVER}/robots.txt`,
  `${SERVER}/llms.txt`,
  `${SERVER}/blog/index.json`,
  `${SERVER}/blog/feed.xml`,
]);

for (const file of walk(join(DIST, 'pages'))) {
  const name = relative(join(DIST, 'pages'), file).replaceAll('\\', '/');
  if (name.startsWith('posts--') && name.endsWith('.html')) {
    urls.add(`${SERVER}/pages/${name}`);
  }
}

for (const file of walk(join(DIST, 'assets/blog'))) {
  const name = relative(join(DIST, 'assets/blog'), file).replaceAll('\\', '/');
  urls.add(`${SERVER}/assets/blog/${name}`);
}

const all = [...urls];
for (let i = 0; i < all.length; i += CHUNK_SIZE) {
  console.log(JSON.stringify({ files: all.slice(i, i + CHUNK_SIZE) }));
}
