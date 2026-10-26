// Deliberate allow-list: never publish the repository root or personal recordings.
import { mkdir, readFile, writeFile, readdir, copyFile, stat } from 'node:fs/promises';
import { createHash } from 'node:crypto';
import { fileURLToPath } from 'node:url';
import path from 'node:path';
const root = fileURLToPath(new URL('../../', import.meta.url));
const out = path.join(root, 'web/public/assets');
await mkdir(out, { recursive: true });
const manifest = { schema: 1, version: '', files: {}, attribution: 'See assets/NOTICE.txt' };
async function emit(name, data) {
  const bytes = Buffer.isBuffer(data) ? data : Buffer.from(data);
  await mkdir(path.dirname(path.join(out, name)), { recursive: true });
  await writeFile(path.join(out, name), bytes);
  manifest.files[name] = { bytes: bytes.length, sha256: createHash('sha256').update(bytes).digest('hex') };
}
const chunks = [Buffer.from('DCAS1'), Buffer.alloc(4)];
let count = 0;
for (const kind of ['shop', 'h_card', 'h_crit']) {
  const dir = path.join(root, 'data/icons', kind);
  const names = (await readdir(dir)).filter(n => n.endsWith('.png')).sort();
  if (!names.length) throw new Error(`No ${kind} templates. Run scripts/sync-assets.ps1 first.`);
  for (const name of names) {
    const bytes = await readFile(path.join(dir, name));
    const label = Buffer.from(`${kind}/${name.slice(0, -4)}`);
    const lengths = Buffer.alloc(6); lengths.writeUInt16LE(label.length); lengths.writeUInt32LE(bytes.length, 2);
    chunks.push(lengths, label, bytes); count++;
  }
}
chunks[1].writeUInt32LE(count);
await emit('templates.bin', Buffer.concat(chunks));
for (const name of ['heroes.json', 'items.json', 'timings.json', 'counters.json', 'rules.json']) {
  const text = (await readFile(path.join(root, 'data', name), 'utf8')).replace(/^\uFEFF/, '');
  JSON.parse(text); await emit(name, text);
}
for (const name of ['text-detection.rten', 'text-rec.rten']) {
  await emit(`models/${name}`, await readFile(path.join(root, 'data/models', name)));
}
const knowledge = [];
for (const folder of ['timings', 'jungle', 'strategy', 'heroes', 'items']) {
  for (const name of (await readdir(path.join(root, 'docs', folder))).filter(n => n.endsWith('.md')).sort()) {
    const body = (await readFile(path.join(root, 'docs', folder, name), 'utf8')).replace(/^\uFEFF/, '');
    knowledge.push({ id: `${folder}/${name}`, category: folder,
      title: body.match(/^#\s+(.+)$/m)?.[1] || name, body });
  }
}
await emit('knowledge.json', JSON.stringify(knowledge));
const profiles = [];
for (const file of (await readdir(path.join(root, 'data/profiles'))).filter(f => f.endsWith('.json')).sort()) {
  const profile = JSON.parse((await readFile(path.join(root, 'data/profiles', file), 'utf8')).replace(/^\uFEFF/, ''));
  if (profile.schema_version !== 1 || !profile.id || !Array.isArray(profile.nodes)) throw new Error(`Invalid game profile ${file}`);
  profiles.push(profile);
}
await emit('profiles.json', JSON.stringify(profiles));
await emit('perception-guide.md', await readFile(path.join(root, 'docs/perception/README.md')));
await emit('laning-policy.json', await readFile(path.join(root,'data/coaching/laning.json')));
// Only the already checked-in, explicitly named regression fixtures are included.
const fixtureNames = ['00014_1792966706577.jpg', '00059_1792966931596.jpg',
  '00184_scoreboard-mid.jpg', '00216_dead-respawn.jpg', '00048_1792966876580.jpg'];
for (const name of fixtureNames) await emit(`fixtures/${name}`, await readFile(path.join(root, 'samples/frames', name)));
await emit('demo.jsonl', await readFile(path.join(root, 'samples/session2-minimap.jsonl')));
await emit('NOTICE.txt', `DeadlockCoach browser assets\n\nOCR: ocrs/RTen (Robert Knight), MIT OR Apache-2.0. Models: https://github.com/robertknight/ocrs-models\nGame artwork: Valve; data and images obtained from https://deadlock-api.com/. No claim of ownership of game artwork.\nWiki-derived knowledge includes citations; wiki text is CC BY-SA (https://deadlock.wiki/). Preserve source attribution and applicable share-alike terms when redistributing derived text.\nKnowledge contains dated, conflicting and unverified claims. Analytics are correlational.\n`);
manifest.version = createHash('sha256').update(JSON.stringify(manifest.files)).digest('hex').slice(0, 16);
await writeFile(path.join(out, 'manifest.json'), JSON.stringify(manifest, null, 2));
console.log(`Assets ${manifest.version}: ${count} templates, ${knowledge.length} knowledge documents, ${Object.keys(manifest.files).length} files.`);
