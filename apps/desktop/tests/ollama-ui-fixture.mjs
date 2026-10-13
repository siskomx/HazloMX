import http from 'node:http';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import assert from 'node:assert/strict';

const selfCheck = process.argv[2] === '--self-check';
assert(selfCheck || process.env.USERNAME === 'WDAGUtilityAccount', 'Owned Sandbox required');
const root = selfCheck ? fs.mkdtempSync(path.join(os.tmpdir(), 'hazlomexa-http-fixture-')) : 'C:/HazloProbe';
const modeFile = selfCheck ? path.join(root, 'mode.txt') : path.join(root, 'input/fixture-mode.txt');
const logFile = selfCheck ? path.join(root, 'requests.json') : path.join(root, 'output/fixture-requests.json');
if (selfCheck) fs.writeFileSync(modeFile, 'missing');
const model = 'qwen3:4b';
const digest = '359d7dd4bcdab3d86b87d73ac27966f4dbb9f5efdfcc75d34a8764a09474fae7';
const requests = [];
function save() { fs.writeFileSync(logFile, JSON.stringify(requests, null, 2)); }
const server = http.createServer((request, response) => {
  const mode = fs.readFileSync(modeFile, 'utf8').trim();
  const entry = { route: request.url, mode, closed: false, received_unix_ms: Date.now() };
  requests.push(entry); if (requests.length > 128) requests.shift(); save();
  request.resume();
  response.setHeader('Content-Type', 'application/json');
  if (request.url === '/api/version') response.end(JSON.stringify({ version: '0.35.1' }));
  else if (request.url === '/api/tags') response.end(JSON.stringify({ models: mode === 'missing' ? [] : [{ name: model, digest, size: 1234 }] }));
  else if (request.url === '/api/show') response.end(JSON.stringify({ capabilities: ['completion'] }));
  else if (request.url === '/api/generate' && mode === 'stall') {
    response.setHeader('Content-Type', 'application/x-ndjson');
    response.write(JSON.stringify({ model, response: '{"status":"proposal",', done: false }) + '\n');
    response.on('close', () => { entry.closed = true; entry.closed_unix_ms = Date.now(); save(); });
  } else if (request.url === '/api/generate' && mode === 'error') {
    response.setHeader('Content-Type', 'application/x-ndjson');
    response.end(JSON.stringify({ error: 'hazlomexa-test-secret-do-not-persist-9bb' }) + '\n');
  } else if (request.url === '/api/generate' && mode === 'clarification') {
    response.setHeader('Content-Type', 'application/x-ndjson');
    response.end(JSON.stringify({ model, response: JSON.stringify({ status: 'clarification', message: '¿Cuánto mide cada lado del cubo y en qué unidades?' }), done: true, done_reason: 'stop' }) + '\n');
  } else { response.statusCode = 404; response.end('{}'); }
});
await new Promise((resolve, reject) => { server.once('error', reject); server.listen(selfCheck ? 0 : 11434, '127.0.0.1', resolve); });
if (selfCheck) {
  try {
    const base = `http://127.0.0.1:${server.address().port}`;
    assert.equal((await (await fetch(base + '/api/version')).json()).version, '0.35.1');
    assert.deepEqual((await (await fetch(base + '/api/tags')).json()).models, []);
    fs.writeFileSync(modeFile, 'stall');
    assert.equal((await (await fetch(base + '/api/tags')).json()).models[0].digest, digest);
    assert.deepEqual((await (await fetch(base + '/api/show', { method: 'POST' })).json()).capabilities, ['completion']);
    const controller = new AbortController();
    const response = await fetch(base + '/api/generate', { method: 'POST', signal: controller.signal });
    assert.match(new TextDecoder().decode((await response.body.getReader().read()).value), /proposal/);
    controller.abort();
    const deadline = Date.now() + 2000;
    while (!requests.at(-1).closed && Date.now() < deadline) await new Promise(resolve => setTimeout(resolve, 10));
    assert.equal(requests.at(-1).closed, true, 'Cancelled client connection did not close');
    for (const mode of ['error', 'clarification']) {
      fs.writeFileSync(modeFile, mode);
      const reply = await fetch(base + '/api/generate', { method: 'POST' });
      assert.equal(reply.headers.get('content-type'), 'application/x-ndjson');
      const record = JSON.parse(await reply.text());
      if (mode === 'error') assert.equal(record.error, 'hazlomexa-test-secret-do-not-persist-9bb');
      else assert.equal(JSON.parse(record.response).status, 'clarification');
    }
    assert(!fs.readFileSync(logFile, 'utf8').includes('hazlomexa-test-secret'));
    console.log('PASS: metadata, partial stream, client disconnect, error and clarification NDJSON');
  } finally { server.closeAllConnections(); await new Promise(resolve => server.close(resolve)); }
} else {
  fs.writeFileSync(path.join(root, 'output/fixture-ready.json'), JSON.stringify({ pid: process.pid, port: 11434, models_run: 0 }));
}
