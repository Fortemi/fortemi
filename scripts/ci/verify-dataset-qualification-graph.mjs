#!/usr/bin/env node
import assert from 'node:assert/strict';
import fs from 'node:fs';
import { fileURLToPath } from 'node:url';

// Validates readiness ordering only. No issue status or receipt is trusted here.
export function validateGraph(graph) {
  assert.equal(graph.schemaVersion, '1.0.0', 'unsupported graph version');
  assert.ok(Array.isArray(graph.nodes) && graph.nodes.length, 'nodes required');
  const nodes = new Map();
  const kinds = new Set(['authority', 'external', 'qualification', 'approval', 'inventory', 'closure']);
  for (const node of graph.nodes) {
    assert.ok(typeof node.id === 'string' && node.id.length, 'node identity required');
    assert.ok(!nodes.has(node.id), `duplicate node ${node.id}`);
    assert.ok(kinds.has(node.kind), `unknown kind ${node.kind}`);
    assert.ok(Array.isArray(node.requires), `dependencies required: ${node.id}`);
    assert.equal(new Set(node.requires).size, node.requires.length, `duplicate dependency: ${node.id}`);
    if (['external', 'qualification', 'closure'].includes(node.kind)) {
      assert.match(node.issue ?? '', /^https:\/\/git\.integrolabs\.net\/[\w.-]+\/[\w.-]+\/issues\/[1-9][0-9]*$/, 'exact Gitea issue required');
    }
    nodes.set(node.id, node);
  }
  const visiting = new Set(), visited = new Set(), order = [];
  function visit(id) {
    assert.ok(nodes.has(id), `unknown dependency ${id}`);
    assert.ok(!visiting.has(id), `readiness cycle at ${id}`);
    if (visited.has(id)) return;
    visiting.add(id);
    for (const dependency of nodes.get(id).requires) visit(dependency);
    visiting.delete(id);
    visited.add(id);
    order.push(id);
  }
  for (const id of nodes.keys()) visit(id);
  const authority = nodes.get('authority-published');
  assert.equal(authority?.kind, 'authority', 'authority publication required');
  assert.equal(authority.requires.length, 0, 'publication must not wait on child closure');
  const closure = nodes.get('epic-closed');
  assert.equal(closure?.kind, 'closure', 'epic closure required');
  function ancestors(id, result = new Set()) {
    for (const dependency of nodes.get(id).requires) {
      if (!result.has(dependency)) { result.add(dependency); ancestors(dependency, result); }
    }
    return result;
  }
  const closing = ancestors('epic-closed');
  for (const node of nodes.values()) {
    if (node.kind === 'qualification') {
      assert.ok(ancestors(node.id).has('authority-published'), `${node.id} bypasses authority`);
      assert.ok(closing.has(node.id), `${node.id} omitted from closure`);
    }
  }
  for (const [target, prerequisites] of Object.entries({
    'fault-hosted': ['tenant', 'fault-nonhosted'],
    restore: ['tenant', 'fault-nonhosted', 'recovery-envelope-approved', 'recovery-surface-inventory'],
    'skew-midrun': ['fault-nonhosted', 'skew-offline'],
    'skew-rollback': ['restore', 'skew-midrun'],
    load: ['tenant', 'fault-nonhosted', 'load-envelope-approved', 'telemetry-ready'],
  })) {
    assert.ok(nodes.has(target), `required qualification missing: ${target}`);
    for (const prerequisite of prerequisites) assert.ok(ancestors(target).has(prerequisite), `${target} bypasses ${prerequisite}`);
  }
  assert.ok(Array.isArray(graph.consumers), 'consumer issue matrix required');
  for (const [repository, issue] of [['roctinam/aiwg', 2242], ['Fortemi/fortemi-react', 412], ['Fortemi/HotM', 231]]) {
    assert.ok(graph.consumers.some(c => c.repository === repository && c.issue === `https://git.integrolabs.net/${repository}/issues/${issue}` && typeof c.condition === 'string' && c.condition.length), `consumer issue missing: ${repository}`);
  }
  return order;
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  const source = process.argv[2] ?? new URL('../../contracts/dataset-qualification/1.0.0/readiness.json', import.meta.url);
  const order = validateGraph(JSON.parse(fs.readFileSync(source, 'utf8')));
  console.log(JSON.stringify({ graphValid: true, qualifiesExecution: false, order }, null, 2));
}
