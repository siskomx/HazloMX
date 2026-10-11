import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync, existsSync } from 'node:fs';
import { execFileSync, spawn } from 'node:child_process';
import { once } from 'node:events';
import { fileURLToPath } from 'node:url';

test('build includes the landing page, referenced assets and deployment headers', () => {
  execFileSync(process.execPath, [fileURLToPath(new URL('./build.mjs', import.meta.url))]);
  const html = readFileSync(new URL('../dist/index.html', import.meta.url), 'utf8');
  assert.match(html, /<title>Hazlo Mexa/);
  assert.match(html, /In planning/);
  for (const [, asset] of html.matchAll(/(?:href|src)="\/([^"#]+)"/g)) {
    assert.ok(existsSync(new URL(`../dist/${asset}`, import.meta.url)), `Missing ${asset}`);
  }
  assert.match(readFileSync(new URL('../dist/_headers', import.meta.url), 'utf8'), /Content-Security-Policy/);
});

test('preview serves assets and refuses private files and unsupported methods', async () => {
  const child = spawn(process.execPath, [fileURLToPath(new URL('./preview.mjs', import.meta.url))], { stdio: ['ignore', 'pipe', 'pipe'] });
  try {
    await Promise.race([once(child.stdout, 'data'), once(child, 'exit').then(() => { throw new Error('Preview exited before startup'); })]);
    const home = await fetch('http://127.0.0.1:4173/');
    assert.equal(home.status, 200);
    assert.match(await home.text(), /Hazlo Mexa/);
    assert.equal((await fetch('http://127.0.0.1:4173/.git/config')).status, 404);
    assert.equal((await fetch('http://127.0.0.1:4173/%2e%2e/LICENSE')).status, 404);
    assert.equal((await fetch('http://127.0.0.1:4173/', { method: 'POST' })).status, 405);
    assert.equal((await fetch('http://127.0.0.1:4173/styles.css', { method: 'HEAD' })).status, 200);
  } finally {
    child.kill();
    await once(child, 'exit');
  }
});
