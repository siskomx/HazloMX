import { createServer } from 'node:http';
import { readFileSync } from 'node:fs';

// Serve only the site's assets; never expose repository files in the preview.
const assets = new Map([
  ['/', ['index.html', 'text/html; charset=utf-8']],
  ['/index.html', ['index.html', 'text/html; charset=utf-8']],
  ['/styles.css', ['styles.css', 'text/css; charset=utf-8']],
  ['/favicon.svg', ['favicon.svg', 'image/svg+xml']],
].map(([path, [file, type]]) => [path, { type, body: readFileSync(new URL(`../public/${file}`, import.meta.url)) }]));

createServer((request, response) => {
  const asset = assets.get((request.url ?? '').split('?')[0]);
  if (!['GET', 'HEAD'].includes(request.method)) {
    response.writeHead(405, { Allow: 'GET, HEAD' }).end();
    return;
  }
  response.writeHead(asset ? 200 : 404, { 'Content-Type': asset?.type ?? 'text/plain', 'X-Content-Type-Options': 'nosniff' });
  response.end(request.method === 'HEAD' ? undefined : asset?.body ?? 'Not found');
}).listen(4173, '127.0.0.1', () => console.log('Preview: http://127.0.0.1:4173'));
