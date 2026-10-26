import { readdir, readFile, writeFile } from 'node:fs/promises';
import { createHash } from 'node:crypto';
import path from 'node:path';
const forbidden = /(^|\/)(sessions|target|\.git|\.env|tag\.txt|agent\.log)(\/|$)|\.(exe|dll|ps1|pem|key)$/i;
async function walk(dir) {
  const files = [];
  for (const e of await readdir(dir, { withFileTypes: true })) {
    const p = path.join(dir, e.name); if (forbidden.test(p.replaceAll('\\', '/'))) throw new Error(`Unexpected published file: ${p}`);
    if (e.isDirectory()) files.push(...await walk(p)); else files.push(p);
  }
  return files;
}
const files = await walk('dist');
if (!files.some(f => f.endsWith('.wasm'))) throw new Error('WASM missing from production artifact');
const manifest = JSON.parse(await readFile('dist/assets/manifest.json', 'utf8'));
for (const [name, entry] of Object.entries(manifest.files)) {
  const data = await readFile(path.join('dist/assets', name));
  const { createHash } = await import('node:crypto');
  if (createHash('sha256').update(data).digest('hex') !== entry.sha256) throw new Error(`Asset integrity mismatch: ${name}`);
}
console.log(`Validated ${files.length} publishable files, WASM and versioned asset integrity.`);
// Generate a scope-relative, versioned offline shell. Models are independently verified
// and cached by the analysis worker; do not duplicate their storage in the shell cache.
const entry = await readFile('dist/index.html', 'utf8');
const version = createHash('sha256').update(entry).update(manifest.version).digest('hex').slice(0, 16);
const shell = files.map(f => f.replaceAll('\\', '/').replace(/^dist\//, ''))
  .filter(f => !f.startsWith('assets/models/') && f !== 'assets/templates.bin' && !f.startsWith('assets/fixtures/'));
await writeFile('dist/sw.js', `
const CACHE = 'deadlock-shell-${version}';
const FILES = ${JSON.stringify(shell)};
self.addEventListener('install', event => event.waitUntil(caches.open(CACHE).then(cache => cache.addAll(FILES.map(f => new URL(f, self.registration.scope).href)))));
self.addEventListener('activate', event => event.waitUntil((async () => {
  for (const key of await caches.keys()) if (key.startsWith('deadlock-shell-') && key !== CACHE) await caches.delete(key);
  await self.clients.claim();
})()));
self.addEventListener('fetch', event => {
  const url = new URL(event.request.url);
  if (event.request.method !== 'GET' || url.origin !== self.location.origin || !url.href.startsWith(self.registration.scope)) return;
  if (event.request.mode === 'navigate') {
    event.respondWith(fetch(event.request).catch(() => caches.open(CACHE).then(c => c.match(new URL('index.html', self.registration.scope).href))));
  } else if (url.pathname.endsWith('/assets/manifest.json')) {
    event.respondWith(fetch(event.request).then(response => { if (response.ok) caches.open(CACHE).then(c => c.put(event.request, response.clone())); return response; }).catch(() => caches.match(event.request, { ignoreVary: true })));
  } else {
    // Only same-origin static assets are cached. Vite's preview server adds Vary: Origin,
    // which otherwise makes the module script's CORS request miss the precached response.
    event.respondWith(caches.match(event.request, { ignoreVary: true }).then(cached => cached || fetch(event.request)));
  }
});
`);
