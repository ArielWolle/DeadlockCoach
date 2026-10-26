import './style.css';
import { Capture, type Pixels } from './capture';
import { defaults, loadSettings, normalize, saveSettings } from './settings';
import { Speaker } from './speech';
import { CropEditor } from './crop-editor';
import { ViewportError, viewport, type Crop } from './crop';
import { sampleBorders } from './borders';
import { loadProfile, saveProfile, isDeadlock, type GameProfile } from './profiles';
import { ProfilePanel } from './profile-panel';
import { clearFrames, download, listFrames, recordFrame, snapshotLines } from './storage';
import type { Analysis, Cue, GameItem, Knowledge, Options, Settings, Snapshot, WorkerReply, ProfileReply, PerceptionReport } from './types';

const $ = <T extends HTMLElement = HTMLElement>(id: string) => document.getElementById(id) as T;
const button = (id: string) => $<HTMLButtonElement>(id);
const input = (id: string) => $<HTMLInputElement>(id);
const text = (id: string, value: string) => { $(id).textContent = value; };
const format = (n?: number | null) => n == null ? '—' : n.toLocaleString();
export const clock = (n?: number | null) => n == null ? '—:—' : `${Math.floor(n / 60)}:${String(n % 60).padStart(2, '0')}`;
const base = new URL(import.meta.env.BASE_URL, location.origin).href;
let settings = loadSettings();
let activeProfile = loadProfile();
const capture = new Capture(), speaker = new Speaker(settings);
capture.configure(activeProfile);
const cropEditor = new CropEditor();
let profilePanel: ProfilePanel | undefined, profileChanging = false, referenceTimestamp = 0;
let latestReport: PerceptionReport | undefined;
let pendingProfile: { id: number; resolve: () => void; reject: (e: Error) => void; timer: ReturnType<typeof setTimeout> } | undefined;
let worker: Worker | undefined, ready = false, initializing: Promise<void> | undefined;
let initResolve: (() => void) | undefined, initReject: ((e: Error) => void) | undefined;
let initTimer: ReturnType<typeof setTimeout> | undefined;
let id = 0, generation = 0, busy = false, running = false, paused = false, replaying = false;
let timer: ReturnType<typeof setTimeout> | undefined;
let lastResultAt = 0, lastElapsed = 0, assetVersion = 'Not loaded';
let needsViewport = false, manualViewport = false, automaticViewport = false, sourceGeometry = '';
let latest: Snapshot = {}, latestFrame: ImageData | undefined, latestMap: ImageData | undefined;
let cues: Array<Cue & { at?: number | null }> = [];
let knowledge: Knowledge[] = [], knowledgeReady: Promise<void> | undefined;
let itemNames: Record<string, string> = {}, heroNames: Record<string, string> = {};
let pending: { id: number; resolve: (r: { analysis: Analysis; elapsed: number }) => void;
  reject: (e: Error) => void; timer: ReturnType<typeof setTimeout> } | undefined;

function notice(message: string, error = false) {
  text('notice', message); $('notice').hidden = !message; $('notice').classList.toggle('error', error);
  if (!message) button('fix-viewport').hidden = true;
}
function status(value: string, source?: string) {
  text('session-state', value); if (source) text('source-label', source);
  $('status-dot').classList.toggle('active', running && !paused);
  button('pause').disabled = !running; button('stop').disabled = !running && !capture.active && !replaying;
  button('start').disabled = running || replaying || profileChanging; button('pause').textContent = paused ? 'Resume' : 'Pause';
  button('edit-viewport').disabled = replaying || profileChanging;
  button('edit-viewport-settings').disabled = replaying || profileChanging;
}
function diagnostics() {
  const values = { 'Engine': ready ? 'Local WASM · single-thread worker' : 'Not initialized', 'Asset version': assetVersion,
    'Secure context': String(window.isSecureContext), 'Screen capture': typeof navigator.mediaDevices?.getDisplayMedia === 'function' ? 'Available' : 'Unavailable',
    'Voice API': window.speechSynthesis ? 'Available' : 'Unavailable', 'Cross-origin isolation': String(window.crossOriginIsolated),
    'Last processing time': lastElapsed ? `${Math.round(lastElapsed)} ms` : '—', 'Recording': settings.record ? 'Local · bounded' : 'Off' };
  $('diagnostics').replaceChildren(...Object.entries(values).flatMap(([key, value]) => {
    const dt = document.createElement('dt'), dd = document.createElement('dd'); dt.textContent = key; dd.textContent = value; return [dt, dd];
  }));
}

function failWorker(message: string) {
  ready = false; initializing = undefined; worker?.terminate(); worker = undefined;
  clearTimeout(initTimer); initReject?.(new Error(message)); initResolve = undefined; initReject = undefined;
  if (pending) { clearTimeout(pending.timer); pending.reject(new Error(message)); pending = undefined; }
  if (pendingProfile) { clearTimeout(pendingProfile.timer); pendingProfile.reject(new Error(message)); pendingProfile = undefined; }
  busy = false; text('engine-state', 'ENGINE ERROR'); $('load-panel').hidden = true;
  if (running) { paused = true; speaker.cancel(); status('Analysis paused', 'Restart the engine in Settings, then resume.'); }
  diagnostics(); notice(message, true);
}

async function initialize(profile = activeProfile) {
  if (ready) return;
  if (initializing) return initializing;
  $('load-panel').hidden = false; text('engine-state', 'ENGINE LOADING');
  initializing = new Promise<void>((resolve, reject) => { initResolve = resolve; initReject = reject; });
  worker = new Worker(new URL('./coach.worker.ts', import.meta.url), { type: 'module' });
  worker.onerror = event => { event.preventDefault(); failWorker(`Analysis worker failed: ${event.message || 'unknown worker error'}`); };
  worker.onmessage = event => {
    const reply: WorkerReply | ProfileReply = event.data;
    if (reply.type === 'profile_ready' || reply.type === 'profile_error') {
      if (pendingProfile?.id === reply.requestId) {
        clearTimeout(pendingProfile.timer); const task = pendingProfile; pendingProfile = undefined;
        if (reply.type === 'profile_ready') task.resolve(); else task.reject(new Error(reply.message));
      }
      return;
    }
    if (reply.type === 'progress') {
      text('load-text', reply.message); $<HTMLProgressElement>('load-progress').value = reply.fraction;
    } else if (reply.type === 'ready') {
      clearTimeout(initTimer); ready = true; assetVersion = reply.version; $('load-panel').hidden = true;
      text('engine-state', 'LOCAL ENGINE READY'); worker?.postMessage({ type: 'configure', options: settings.options });
      initResolve?.(); initResolve = undefined; initReject = undefined; diagnostics();
    } else if (reply.type === 'error') {
      // WASM traps may leave the instance unusable; rebuild rather than replaying bad state.
      failWorker(reply.message);
    } else if (reply.type === 'result' && pending?.id === reply.id) {
      clearTimeout(pending.timer); const task = pending; pending = undefined;
      if (reply.generation !== generation) task.reject(new Error('Analysis canceled'));
      else task.resolve({ analysis: reply.analysis, elapsed: reply.elapsed });
    }
  };
  initTimer = setTimeout(() => failWorker('Model initialization timed out. Check connectivity, then restart the engine.'), 120000);
  worker.postMessage({ type: 'init', base, profile });
  return initializing;
}

function request(message: Record<string, unknown>, transfer: Transferable[] = []) {
  if (!ready || !worker) return Promise.reject(new Error('Engine is not ready'));
  if (pending) return Promise.reject(new Error('Another analysis is still running'));
  const requestId = ++id;
  return new Promise<{ analysis: Analysis; elapsed: number }>((resolve, reject) => {
    pending = { id: requestId, resolve, reject, timer: setTimeout(() => failWorker('Frame analysis exceeded 45 seconds. Capture is paused; restart the engine to recover.'), 45000) };
    worker!.postMessage({ ...message, id: requestId, generation }, transfer);
  });
}

function cancelMode() {
  generation++; running = false; paused = false; replaying = false; clearTimeout(timer);
  cropEditor.cancel(); needsViewport = false; button('fix-viewport').hidden = true;
  capture.stop(); speaker.cancel(); button('cancel-replay').disabled = true;
  for (const key of ['demo', 'demo-replay', 'replay-import']) button(key).disabled = false;
  // A worker cannot cancel synchronous WASM inference. Terminate an in-flight instance
  // rather than allowing old frames/reset commands to race with the next session.
  if (pending || pendingProfile || (!ready && initializing)) {
    const canceled = new Error('Analysis canceled');
    if (pending) { clearTimeout(pending.timer); pending.reject(canceled); pending = undefined; }
    if (pendingProfile) { clearTimeout(pendingProfile.timer); pendingProfile.reject(canceled); pendingProfile = undefined; }
    clearTimeout(initTimer); initReject?.(canceled); initReject = undefined; initResolve = undefined;
    worker?.terminate(); worker = undefined; ready = false; initializing = undefined; busy = false;
    $('load-panel').hidden = true; text('engine-state', 'ENGINE OFFLINE');
  }
  text('feed-badge', 'CAPTURE STOPPED'); status('Ready to connect', 'Choose the Deadlock game window to begin');
}
function resetEngine() {
  worker?.postMessage({ type: 'reset' }); latest = {}; lastResultAt = 0;
  latestReport = undefined; profilePanel?.clear();
  renderSnapshot(latest);
}

async function autoCropSource(current: number): Promise<boolean> {
  if (!settings.autoCrop || manualViewport || !capture.active) return false;
  const size = { width: capture.video.videoWidth, height: capture.video.videoHeight };
  if (!size.width || !size.height) return false;
  try {
    const crop = await sampleBorders(capture.video, size, activeProfile.viewport.aspect_ratio);
    if (current !== generation || !capture.active) return false;
    if (crop) { settings.crop = crop; automaticViewport = true; }
    else if (automaticViewport && (activeProfile.viewport.aspect_ratio === null || Math.abs(size.width / size.height - activeProfile.viewport.aspect_ratio) <= .06)) {
      settings.crop = { x: 0, y: 0, w: 1, h: 1 }; automaticViewport = false;
    } else return false;
    saveSettings(settings); populateSettings(); return true;
  } catch { return false; } // A resizing/muted source can be calibrated manually instead.
}

async function editLiveViewport(required = needsViewport, invalidate = true): Promise<boolean> {
  if (!capture.active) { void start(true); return false; }
  if (cropEditor.active) return false;
  if (invalidate) generation++;
  const current = generation, wasPaused = paused;
  if (running) paused = true;
  clearTimeout(timer); speaker.cancel();
  needsViewport = required; button('fix-viewport').hidden = !required;
  status('Adjusting game viewport', 'Drag the frame around the game; analysis is paused.');
  const crop = await cropEditor.open({ image: capture.video, width: capture.video.videoWidth, height: capture.video.videoHeight,
    initial: settings.crop, description: 'This is the full shared source, updating live. Select only the game video and keep its HUD inside the frame.',
    aspect: activeProfile.viewport.aspect_ratio,
    applyLabel: required ? (running ? 'Apply and resume coaching' : 'Apply and start coaching') : 'Apply viewport' });
  if (current !== generation || !capture.active) return false;
  if (!crop) {
    if (running) {
      paused = required || wasPaused;
      status(paused ? 'Coaching paused' : 'Coaching live');
      if (!paused) schedule(0);
    }
    if (required) { notice('Select the game area to continue coaching.', true); button('fix-viewport').hidden = false; }
    return false;
  }
  settings.crop = crop; manualViewport = true; automaticViewport = false; needsViewport = false;
  sourceGeometry = `${capture.video.videoWidth}x${capture.video.videoHeight}`;
  saveSettings(settings); populateSettings(); button('fix-viewport').hidden = true;
  resetEngine();
  if (running) {
    paused = required ? false : wasPaused;
    if (!paused && !ready) {
      status('Restarting analysis…');
      try { await initialize(); } catch { paused = true; status('Analysis paused'); return true; }
      if (current !== generation || !capture.active) return true;
    }
    status(paused ? 'Coaching paused' : 'Coaching live'); if (!paused) schedule(0);
  }
  notice('Game viewport applied. Only the selected area will be analyzed.');
  return true;
}

async function start(forceEditor = false) {
  if (running || replaying || profileChanging) return;
  generation++; const current = generation; notice('');
  // Screen permission is requested before any asynchronous model work.
  const sharing = capture.start();
  status('Opening screen picker…'); button('start').disabled = true;
  try {
    const source = await sharing;
    if (current !== generation) { capture.stop(); return; }
    manualViewport = false; automaticViewport = false; sourceGeometry = `${source.width}x${source.height}`;
    status('Checking capture borders…', source.label);
    await autoCropSource(current);
    if (current !== generation) return;
    let invalid = false;
    try { viewport(settings.crop, source, activeProfile.viewport.aspect_ratio); } catch (e) { if (e instanceof ViewportError) invalid = true; else throw e; }
    if (forceEditor || invalid) {
      const applied = await editLiveViewport(true, false);
      if (current !== generation) return;
      if (!applied) { cancelMode(); notice('Capture stopped. Select a source again when you are ready to calibrate.'); return; }
    }
    status('Loading analysis…', source.label); await initialize();
    if (current !== generation) { capture.stop(); return; }
    resetEngine(); running = true; paused = false;
    status('Coaching live', `${source.label} · ${source.width}×${source.height}`);
    text('feed-badge', 'LIVE · LOCAL ANALYSIS'); $('preview-empty').hidden = true;
    if (!window.speechSynthesis) notice('Speech is unavailable in this browser. Advice will still appear on screen.');
    schedule(0);
  } catch (e) { capture.stop(); status('Ready to connect'); notice(String(e instanceof Error ? e.message : e), true); }
}
function schedule(delay = settings.interval * 1000) { clearTimeout(timer); if (running) timer = setTimeout(() => { void tick(); }, delay); }
async function analyzePixels(p: Pixels) {
  drawPixels(p);
  return request({ type: 'frame', rgba: p.rgba.data.buffer, width: p.rgba.width, height: p.rgba.height,
    map: p.map.data.buffer, mapWidth: p.map.width, mapHeight: p.map.height, timestamp: p.timestamp },
    [p.rgba.data.buffer as ArrayBuffer, p.map.data.buffer as ArrayBuffer]);
}
async function tick() {
  if (!running) return;
  if (paused || busy || pending) { schedule(); return; }
  busy = true; const current = generation; const begin = performance.now();
  try {
    const geometry = `${capture.video.videoWidth}x${capture.video.videoHeight}`;
    if (geometry !== sourceGeometry && capture.video.videoWidth && capture.video.videoHeight) {
      sourceGeometry = geometry;
      if (await autoCropSource(current)) resetEngine();
      if (current !== generation) return;
    }
    const pixels = await capture.frame(settings.crop, settings.record);
    const timestamp = pixels.timestamp; const recording = settings.record; const preview = pixels.preview;
    const result = await analyzePixels(pixels);
    if (current !== generation || !running || paused) return;
    if (Date.now() - timestamp > Math.max(15000, settings.interval * 2000)) {
      speaker.cancel(); notice('Analysis is behind the game. This frame’s speech was discarded. Try a longer interval.');
    } else { for (const cue of result.analysis.cues) speaker.say(cue); }
    apply(result.analysis, result.elapsed); status('Coaching live');
    if (recording && preview) {
      try { await recordFrame({ created: Date.now(), timestamp, snapshot: result.analysis.snapshot, cues: result.analysis.cues, image: preview,
        perception: result.analysis.perception, profile_id: activeProfile.id }); }
      catch (e) { settings.record = false; input('record').checked = false; saveSettings(settings); notice(`Local recording paused: ${String(e)}. Coaching continues.`, true); }
    }
  } catch (e) {
    if (current === generation) {
      speaker.cancel(); notice(e instanceof Error ? e.message : String(e), true);
      if (e instanceof ViewportError) { needsViewport = true; await editLiveViewport(true); }
      else status('Waiting for a readable frame');
    }
  } finally {
    busy = false;
    if (current === generation) schedule(Math.max(200, settings.interval * 1000 - (performance.now() - begin)));
  }
}

function drawPixels(p: Pixels) {
  // Save a copy before transferring the analysis buffers to the worker.
  latestFrame = new ImageData(new Uint8ClampedArray(p.rgba.data), p.rgba.width, p.rgba.height);
  latestMap = new ImageData(new Uint8ClampedArray(p.map.data), p.map.width, p.map.height);
  $('preview-empty').hidden = true; drawPreview(); drawMap(latest);
  text('frame-info', `${p.rgba.width}×${p.rgba.height} frame${isDeadlock(activeProfile) ? ` · ${p.map.width}×${p.map.height} minimap` : ` · ${activeProfile.name}`}`);
}
function drawPreview() {
  const canvas = $<HTMLCanvasElement>('preview'), ctx = canvas.getContext('2d')!;
  if (latestFrame) {
    canvas.width = latestFrame.width; canvas.height = latestFrame.height;
    canvas.parentElement!.style.aspectRatio = `${latestFrame.width}/${latestFrame.height}`;
  }
  ctx.clearRect(0, 0, canvas.width, canvas.height);
  if (!latestFrame) return;
  ctx.putImageData(latestFrame, 0, 0);
  if (!input('overlay').checked) return;
  ctx.strokeStyle = '#82d2c1'; ctx.lineWidth = 2;
  if (isDeadlock(activeProfile)) for (const [x, y, w, h] of [[752, 9, 96, 32], [64, 720, 144, 54], [20, 788, 304, 98], [1255, 525, 335, 350]]) ctx.strokeRect(x, y, w, h);
  else for (const s of latestReport?.stages || []) if (s.operation === 'crop' && s.bounds) ctx.strokeRect(s.bounds.x, s.bounds.y, s.bounds.w, s.bounds.h);
}
function drawMap(s: Snapshot) {
  const canvas = $<HTMLCanvasElement>('minimap'), ctx = canvas.getContext('2d')!;
  ctx.fillStyle = '#10191c'; ctx.fillRect(0, 0, canvas.width, canvas.height);
  if (latestMap) {
    const off = document.createElement('canvas'); off.width = latestMap.width; off.height = latestMap.height;
    off.getContext('2d')!.putImageData(latestMap, 0, 0); ctx.drawImage(off, 0, 0, canvas.width, canvas.height);
  } else {
    ctx.strokeStyle = '#354a48'; ctx.lineWidth = 2;
    for (const x of [130, 335, 540]) { ctx.beginPath(); ctx.moveTo(335, 650); ctx.quadraticCurveTo(x, 370, 335, 70); ctx.stroke(); }
    ctx.fillStyle = '#b5c8c0'; ctx.font = '18px system-ui'; ctx.fillText('Snapshot positions · no captured image', 50, 35);
  }
  const map = s.minimap;
  if (map) {
    for (const p of map.enemies || []) { ctx.strokeStyle = '#ff7364'; ctx.lineWidth = 3; ctx.beginPath(); ctx.arc(p.x * 670, p.y * 700, 17, 0, Math.PI * 2); ctx.stroke(); }
    if (map.urn_pos) { ctx.strokeStyle = '#69eef0'; ctx.lineWidth = 3; ctx.strokeRect(map.urn_pos.x * 670 - 11, map.urn_pos.y * 700 - 16, 22, 32); }
  }
}

function apply(analysis: Analysis, elapsed: number) {
  latest = analysis.snapshot; lastResultAt = Date.now(); lastElapsed = elapsed;
  renderSnapshot(latest); text('analysis-time', `${Math.round(elapsed)} ms · ${replaying ? 'replay' : 'local processing'}`);
  if (analysis.perception) {
    latestReport = analysis.perception; profilePanel?.render(analysis.perception); drawPreview();
    window.dispatchEvent(new CustomEvent('coach:perception', { detail: structuredClone(analysis.perception) }));
  }
  for (const cue of analysis.cues) {
    cues.unshift({ ...cue, at: latest.game_clock_secs }); cues = cues.slice(0, 60);
    text('current-advice', cue.text); text('advice-context', `${cue.priority.toUpperCase()} PRIORITY · ${clock(latest.game_clock_secs)} · ${cue.key.startsWith('map:') ? 'Minimap evidence' : 'Screen and rule evidence'}`);
  }
  renderCues(); diagnostics();
}
function renderCues() {
  const list = $('cue-list'); list.replaceChildren();
  if (!cues.length) { const li = document.createElement('li'); li.className = 'empty-row'; li.textContent = 'No calls yet. Good advice is worth waiting for.'; list.append(li); }
  for (const cue of cues.slice(0, 15)) {
    const li = document.createElement('li'), time = document.createElement('time'), priority = document.createElement('span'), body = document.createElement('span');
    time.textContent = clock(cue.at); priority.textContent = cue.priority.toUpperCase(); priority.className = `priority ${cue.priority.toLowerCase()}`;
    body.textContent = cue.text; li.append(time, priority, body); list.append(li);
  }
}
function label(className: string, dict: Record<string, string>) { return dict[className] || className.replace(/^(upgrade_|hero_)/, '').replaceAll('_', ' '); }
function renderItems(id: string, items: string[] = []) {
  const list = $(id); list.replaceChildren();
  for (const item of items.length ? items : ['Nothing confidently observed.']) { const li = document.createElement('li'); li.textContent = label(item, itemNames); list.append(li); }
}
function renderSnapshot(s: Snapshot) {
  text('metric-clock', clock(s.game_clock_secs)); text('metric-souls', format(s.souls));
  text('metric-screen', s.dead ? `Dead · respawn ${s.respawn_secs ?? '?'}s` : s.screen || 'Waiting for gameplay');
  const a = s.ally_networth, e = s.enemy_networth;
  text('metric-lead', a != null && e != null ? `${a >= e ? '+' : ''}${format(a - e)}` : '—');
  text('metric-totals', `${format(a)} / ${format(e)} souls`);
  const map = s.minimap; text('metric-map', map ? `${map.observations}/3 reads` : '—');
  text('map-evidence', map ? `${map.observations} RECENT OBSERVATIONS` : 'NO OBSERVATIONS');
  text('boss-status', map?.midboss_up ? 'Gold icon confirmed' : 'Not confirmed (not necessarily dead)');
  const urnNames = { Visible: 'Marker seen · possession unknown', Ground: 'Stationary marker at bridge', CarriedByYou: 'You are carrying', CarriedByAlly: 'Moving with ally · inferred', CarriedByEnemy: 'Moving with enemy · inferred' };
  text('urn-status', map?.urn ? urnNames[map.urn] : 'Not confirmed'); text('enemy-count', map ? String(map.enemy_count) : '—');
  $('lane-cards').replaceChildren();
  for (const name of ['green', 'blue', 'yellow'] as const) {
    const l = map?.[name]; const card = document.createElement('article'); card.className = `lane-card ${name}`;
    const heading = document.createElement('h2'); heading.textContent = `${name[0].toUpperCase() + name.slice(1)} lane`;
    const front = document.createElement('div'); front.className = 'lane-front'; const fill = document.createElement('span');
    fill.style.width = `${Math.min(100, Math.max(0, (l?.push ?? 0) * 100))}%`; front.append(fill);
    const p = document.createElement('p'); p.textContent = l ? `Front estimate: ${l.push == null ? 'not stable' : `${Math.round(l.push * 100)}% up lane`} · Allied dots ${l.ally_troopers} · Enemy dots ${l.enemy_troopers}` : 'Waiting for minimap observations';
    card.append(heading, front, p); $('lane-cards').append(card);
  }
  renderItems('own-items', s.items); renderItems('enemy-items', s.enemy_items);
  $('roster').replaceChildren();
  for (const [team, heroes] of [['Your team', s.ally_heroes], ['Enemy team', s.enemy_heroes]] as const) {
    const div = document.createElement('div'), heading = document.createElement('h3'); heading.textContent = team; div.append(heading);
    for (const h of heroes?.length ? heroes : ['No confident hero observations yet.']) { const p = document.createElement('p'); p.textContent = label(h, heroNames); div.append(p); }
    $('roster').append(div);
  }
  text('snapshot-json', JSON.stringify(s, null, 2)); drawMap(s);
}

async function loadNames() {
  try {
    const [items, heroes] = await Promise.all([fetch(`${base}assets/items.json`), fetch(`${base}assets/heroes.json`)]);
    if (!items.ok || !heroes.ok) throw new Error('Game data is unavailable; run the asset build.');
    itemNames = Object.fromEntries((await items.json() as GameItem[]).map(i => [i.class_name, i.name]));
    heroNames = Object.fromEntries((await heroes.json() as { class_name: string; name: string }[]).map(h => [h.class_name, h.name]));
  } catch (e) { notice(String(e), true); }
}

async function beginReplay() {
  if (profileChanging) throw new Error('Wait for the profile change to finish');
  if (replaying) throw new Error('A replay is already running');
  cancelMode(); notice(''); generation++; replaying = true; button('cancel-replay').disabled = false;
  for (const key of ['demo', 'demo-replay', 'replay-import']) button(key).disabled = true;
  button('start').disabled = true; text('feed-badge', 'REPLAY · SILENT'); status('Replay lab', 'Local playback · speech is disabled');
  await initialize(); if (!replaying) throw new Error('Replay canceled');
  resetEngine(); cues = []; renderCues(); $('replay-results').replaceChildren(); latestFrame = undefined; latestMap = undefined; drawPreview();
  return generation;
}
function resultRow(name: string, analysis: Analysis, elapsed: number) {
  const div = document.createElement('div'); div.className = 'replay-result';
  const b = document.createElement('button'); b.className = 'quiet'; b.textContent = `${name} · ${clock(analysis.snapshot.game_clock_secs)} · ${analysis.snapshot.screen || 'snapshot'} · ${format(analysis.snapshot.souls)} souls · ${Math.round(elapsed)} ms`;
  b.onclick = () => { latest = analysis.snapshot; renderSnapshot(latest); };
  div.append(b); if (analysis.cues.length) { const p = document.createElement('p'); p.textContent = analysis.cues.map(c => c.text).join(' '); div.append(p); }
  $('replay-results').prepend(div); while ($('replay-results').childElementCount > 100) $('replay-results').lastElementChild?.remove();
}
async function replaySnapshots(snapshots: Snapshot[], current: number) {
  if (!snapshots.length) throw new Error('No snapshots found');
  if (snapshots.length > 2000) throw new Error('Import at most 2,000 snapshots at a time');
  for (let i = 0; i < snapshots.length && current === generation && replaying; i++) {
    const observation = snapshots[i] as Snapshot & Partial<PerceptionReport>;
    if (observation.schema_version === 1 && typeof observation.profile_id === 'string' && observation.fields && !Array.isArray(observation.fields)) {
      const valid = Object.values(observation.fields).every(r => r && ['ok','missing','pending','skipped','error'].includes(r.status));
      if (!valid) throw new Error('Invalid observation field status');
      const analysis: Analysis = { snapshot: {}, cues: [], perception: { ...observation, stages: observation.stages || [] } as PerceptionReport };
      apply(analysis, 0); resultRow(`Observation ${i + 1} (${observation.profile_id})`, analysis, 0); continue;
    }
    if (!isDeadlock(activeProfile)) throw new Error('Legacy snapshots require the Deadlock adapter. Import screenshots or perception JSONL for this profile.');
    const r = await request({ type: 'replay', snapshot: snapshots[i] });
    if (current !== generation) return;
    apply(r.analysis, r.elapsed); resultRow(`Snapshot ${i + 1}`, r.analysis, r.elapsed);
    text('replay-status', `Replaying ${i + 1} / ${snapshots.length} snapshots · silent`);
  }
}
async function demo() {
  let current = -1;
  try {
    current = await beginReplay();
    const response = await fetch(`${base}assets/demo.jsonl`); if (!response.ok) throw new Error(`Demo: HTTP ${response.status}`);
    await replaySnapshots(snapshotLines(await response.text()), current);
    if (current === generation) { text('replay-status', 'Included match replay complete. Select a row to inspect its snapshot.'); notice('Recorded match replayed locally. Switch to Map intelligence to inspect its final observations.'); }
  } catch (e) { if (replaying) notice(String(e), true); }
  finally { if (current === generation || (current === -1 && replaying)) endReplay(); }
}
function endReplay() {
  replaying = false; button('cancel-replay').disabled = true; button('start').disabled = running;
  for (const key of ['demo', 'demo-replay', 'replay-import']) button(key).disabled = false;
  if (!running) status('Replay complete', 'Start coaching to capture your game, or import more frames');
}
async function importFiles() {
  const files = [...input('replay-files').files || []];
  if (!files.length) { notice('Choose screenshots or a recording to analyze.'); return; }
  if (files.length > 120) { notice('Choose at most 120 files at a time.', true); return; }
  let current = -1;
  let importCrop = { ...settings.crop };
  try {
    current = await beginReplay();
    for (const file of files.sort((a, b) => a.name.localeCompare(b.name))) {
      if (current !== generation || !replaying) break;
      const importLimit = /\.json$/i.test(file.name) ? 80 : 20;
      if (file.size > importLimit * 1024 * 1024) throw new Error(`${file.name} exceeds the ${importLimit} MB import limit`);
      if (/\.(jpg|jpeg|png)$/i.test(file.name)) {
        const calibrated = await replayImage(file, importCrop);
        if (!calibrated || current !== generation) return;
        const { pixels: p, crop } = calibrated; importCrop = crop;
        // Imported numbered screenshots are sampled at 5-second virtual spacing.
        p.timestamp = files.indexOf(file) * 5000;
        const r = await analyzePixels(p); if (current !== generation) break;
        apply(r.analysis, r.elapsed); resultRow(file.name, r.analysis, r.elapsed);
        text('replay-status', `Analyzed ${file.name} · silent`);
      } else if (/\.jsonl$/i.test(file.name)) {
        await replaySnapshots(snapshotLines(await file.text()), current);
      } else if (/\.json$/i.test(file.name)) {
        const data = JSON.parse(await file.text());
        if (data.schema !== 1 || !Array.isArray(data.frames) || data.frames.length > 120) throw new Error('Invalid recording export');
        for (const [i, frame] of data.frames.entries()) {
          if (current !== generation || !replaying) break;
          const snapshot = snapshotLines(JSON.stringify(frame.snapshot))[0];
          if (frame.image) {
            if (typeof frame.image !== 'string' || !/^data:image\/(jpeg|png);base64,[A-Za-z0-9+/=]+$/.test(frame.image)) throw new Error(`Frame ${i + 1} has an invalid image`);
            const response = await fetch(frame.image); const blob = await response.blob();
            const pixels = await capture.image(blob, { x: 0, y: 0, w: 1, h: 1 }); drawPixels(pixels);
          }
          if (!isDeadlock(activeProfile) && frame.perception) {
            const analysis: Analysis = { snapshot: {}, cues: [], perception: frame.perception };
            apply(analysis, 0); resultRow(`Recording frame ${i + 1} (observations)`, analysis, 0); continue;
          }
          if (!isDeadlock(activeProfile)) throw new Error('This archive contains legacy snapshots without generic observations. Select its game adapter to replay it.');
          const r = await request({ type: 'replay', snapshot });
          if (current !== generation) break;
          apply(r.analysis, r.elapsed); resultRow(`Recording frame ${i + 1}`, r.analysis, r.elapsed);
        }
      } else { throw new Error(`Unsupported file: ${file.name}`); }
    }
    if (current === generation) text('replay-status', 'Replay complete. Select a result to inspect it.');
  } catch (e) { if (replaying) notice(e instanceof Error ? e.message : String(e), true); }
  finally { if (current === generation || (current === -1 && replaying)) endReplay(); }
}

async function replayImage(file: Blob, crop: Crop): Promise<{ pixels: Pixels; crop: Crop } | undefined> {
  try { return { pixels: await capture.image(file, crop), crop }; }
  catch (e) {
    if (!(e instanceof ViewportError)) throw e;
    const bitmap = await createImageBitmap(file);
    try {
      const detected = settings.autoCrop ? await sampleBorders(bitmap, { width: bitmap.width, height: bitmap.height }, activeProfile.viewport.aspect_ratio) : undefined;
      const selected = detected || await cropEditor.open({ image: bitmap, width: bitmap.width, height: bitmap.height,
        initial: crop, description: 'Select the game inside this screenshot. This crop applies to this import, without changing your live-capture preferences.',
        aspect: activeProfile.viewport.aspect_ratio,
        applyLabel: 'Apply to screenshots' });
      if (!selected) { cancelMode(); text('replay-status', 'Replay canceled.'); return; }
      return { pixels: await capture.image(file, selected), crop: selected };
    } finally { bitmap.close(); }
  }
}

async function recordings() {
  try { const frames = await listFrames(); text('recording-stats', `${frames.length} frames · ${(frames.reduce((n, f) => n + f.image.size, 0) / 1048576).toFixed(1)} MB · stored in this browser`); return frames; }
  catch (e) { text('recording-stats', `Local storage unavailable: ${String(e)}`); return []; }
}
function blobDataURL(blob: Blob): Promise<string> {
  return new Promise((resolve, reject) => { const r = new FileReader(); r.onload = () => resolve(String(r.result)); r.onerror = () => reject(r.error); r.readAsDataURL(blob); });
}

async function loadKnowledge() {
  if (!knowledgeReady) knowledgeReady = (async () => {
    const response = await fetch(`${base}assets/knowledge.json`); if (!response.ok) throw new Error(`Guide: HTTP ${response.status}`);
    knowledge = await response.json(); renderKnowledge();
  })().catch(e => { knowledgeReady = undefined; notice(String(e), true); });
  return knowledgeReady;
}
function renderKnowledge() {
  const q = input('knowledge-search').value.trim().toLowerCase(), category = $<HTMLSelectElement>('knowledge-category').value;
  const results = knowledge.filter(k => (!category || k.category === category) && `${k.title} ${k.body}`.toLowerCase().includes(q));
  $('knowledge-results').replaceChildren();
  for (const k of results.slice(0, 80)) {
    const b = document.createElement('button'); b.className = 'guide-link'; b.textContent = k.title;
    const span = document.createElement('span'); span.textContent = k.category.toUpperCase(); b.append(span);
    b.onclick = () => { text('knowledge-title', k.title); text('knowledge-body', k.body); };
    $('knowledge-results').append(b);
  }
  if (!results.length) { const p = document.createElement('p'); p.textContent = 'No guides match that search.'; $('knowledge-results').append(p); }
}

function voices() {
  const select = $<HTMLSelectElement>('voice'); select.replaceChildren(new Option('Automatic — prefer local English voice', ''));
  for (const v of speaker.voices()) select.append(new Option(`${v.name} · ${v.lang}${v.localService ? ' · local' : ' · service'}`, v.voiceURI));
  select.value = settings.voice;
}
function populateSettings() {
  voices(); input('rate').value = String(settings.rate); input('volume').value = String(settings.volume);
  input('interval').value = String(settings.interval); input('record').checked = settings.record;
  input('auto-crop').checked = settings.autoCrop;
  for (const key of Object.keys(settings.options) as Array<keyof Options>) input(`opt-${key}`).checked = settings.options[key];
  for (const key of ['x', 'y', 'w', 'h'] as const) input(`crop-${key}`).value = String(Math.round(settings.crop[key] * 1000000) / 10000);
  text('rate-value', `${settings.rate.toFixed(1)}×`); text('volume-value', `${Math.round(settings.volume * 100)}%`);
  button('mute').textContent = settings.muted ? 'Unmute voice' : 'Mute voice'; button('mute').setAttribute('aria-pressed', String(settings.muted));
}
function saveForm() {
  const next = structuredClone(settings);
  next.voice = $<HTMLSelectElement>('voice').value; next.rate = Number(input('rate').value); next.volume = Number(input('volume').value);
  next.interval = Number(input('interval').value); next.record = input('record').checked;
  next.autoCrop = input('auto-crop').checked;
  for (const key of Object.keys(next.options) as Array<keyof Options>) next.options[key] = input(`opt-${key}`).checked;
  for (const key of ['x', 'y', 'w', 'h'] as const) next.crop[key] = Number(input(`crop-${key}`).value) / 100;
  if (next.crop.x + next.crop.w > 1.001 || next.crop.y + next.crop.h > 1.001) { notice('Calibration extends beyond the captured source. Offset + size must be no more than 100%.', true); return; }
  if (JSON.stringify(next.crop) !== JSON.stringify(settings.crop)) { manualViewport = true; automaticViewport = false; }
  settings = normalize(next); saveSettings(settings); speaker.configure(settings); worker?.postMessage({ type: 'configure', options: settings.options });
  populateSettings(); diagnostics(); notice('Settings saved on this device.');
}
function route() {
  let page = location.hash.slice(1) || 'coach';
  if (!['coach', 'map', 'match', 'replay', 'pipeline', 'knowledge', 'settings'].includes(page)) page = 'coach';
  if (!isDeadlock(activeProfile) && ['map', 'match', 'knowledge'].includes(page)) page = 'coach';
  document.querySelectorAll<HTMLElement>('.page').forEach(el => { el.hidden = el.id !== `page-${page}`; });
  document.querySelectorAll<HTMLAnchorElement>('[data-page]').forEach(a => { if (a.dataset.page === page) a.setAttribute('aria-current', 'page'); else a.removeAttribute('aria-current'); });
  if (page === 'knowledge') void loadKnowledge(); if (page === 'replay') void recordings(); if (page === 'settings') diagnostics();
}

function refreshGameLayout() {
  const legacy = isDeadlock(activeProfile);
  for (const page of ['map', 'match', 'knowledge']) document.querySelector<HTMLElement>(`[data-page="${page}"]`)!.hidden = !legacy;
  document.querySelector<HTMLElement>('.metrics')!.hidden = !legacy;
  $('generic-observations').hidden = legacy;
  text('active-game-profile', `${activeProfile.name} · game-independent perception outputs`);
  document.querySelector('.visual-calibration-controls p')!.textContent = `Drag the selection around the game. Automatic borders follow the active profile: ${activeProfile.viewport.aspect_ratio === null ? 'any aspect ratio' : 'fixed aspect ratio'}.`;
  for (const key of ['timers', 'map', 'economy', 'counters', 'deaths']) input(`opt-${key}`).disabled = !legacy;
  if (!legacy) {
    text('current-advice', 'Reading your configured UI.');
    text('advice-context', 'Generic perception profile: observations are published without Deadlock-specific coaching.');
  }
  profilePanel?.updateActive(); route();
}

async function applyGameProfile(profile: GameProfile) {
  if (profileChanging || replaying || cropEditor.active) throw new Error('Finish the current replay/editor operation before changing profiles');
  const wasRunning = running, wasPaused = paused;
  profileChanging = true; generation++; const current = generation;
  if (running) paused = true;
  clearTimeout(timer); speaker.cancel(); status('Preparing game profile…');
  $('load-panel').hidden = false;
  try {
    // If a previous initialization was already in progress, finish it before replacing it.
    if (initializing && !ready) await initializing;
    if (!ready) await initialize(profile);
    else await new Promise<void>((resolve, reject) => {
      const requestId = ++id;
      pendingProfile = { id: requestId, resolve, reject, timer: setTimeout(() => failWorker('Profile initialization timed out.'), 120000) };
      worker!.postMessage({ type: 'profile', profile, base, requestId });
    });
    activeProfile = profile; capture.configure(profile); manualViewport = false; automaticViewport = false;
    worker?.postMessage({ type: 'configure', options: settings.options });
    resetEngine(); refreshGameLayout();
    const saved = saveProfile(profile);
    if (current === generation && wasRunning && capture.active) {
      paused = wasPaused;
      sourceGeometry = `${capture.video.videoWidth}x${capture.video.videoHeight}`;
      await autoCropSource(current);
      let valid = true;
      try { viewport(settings.crop, { width: capture.video.videoWidth, height: capture.video.videoHeight }, activeProfile.viewport.aspect_ratio); }
      catch { valid = false; }
      if (!valid) await editLiveViewport(true);
      else { status(paused ? 'Coaching paused' : 'Coaching live'); if (!paused) schedule(0); }
    } else if (!running) status('Profile ready', `${profile.name} · select a source or load a reference image`);
    notice(saved ? `Active perception profile: ${profile.name}` : 'Profile active for this session. Browser storage is full; export the profile to keep it.');
  } catch (e) {
    if (current === generation && wasRunning && capture.active) {
      paused = ready ? wasPaused : true; status(paused ? 'Analysis paused' : 'Coaching live'); if (!paused) schedule(0);
    } else if (!running) status('Profile unchanged', activeProfile.name);
    throw e;
  } finally {
    profileChanging = false; $('load-panel').hidden = true;
    status($('session-state').textContent || 'Ready to connect');
  }
}

async function loadReferenceImage(file: File) {
  if (file.size > 20 * 1024 * 1024) throw new Error('Reference image exceeds 20 MB');
  if (running && !paused) { generation++; paused = true; clearTimeout(timer); speaker.cancel(); status('Coaching paused', 'Reference image loaded for pipeline authoring.'); }
  const bitmap = await createImageBitmap(file);
  try {
    const scale = Math.min(1, activeProfile.viewport.max_width / bitmap.width, 2304 / bitmap.height);
    const canvas = document.createElement('canvas'); canvas.width = Math.round(bitmap.width * scale); canvas.height = Math.round(bitmap.height * scale);
    const ctx = canvas.getContext('2d')!; ctx.drawImage(bitmap, 0, 0, canvas.width, canvas.height);
    resetEngine();
    latestFrame = ctx.getImageData(0, 0, canvas.width, canvas.height); latestReport = undefined; referenceTimestamp = 0;
    $('preview-empty').hidden = true; text('feed-badge', 'REFERENCE · LOCAL'); drawPreview();
  } finally { bitmap.close(); }
}

async function pickFieldRegion(initial: Crop) {
  if (!latestFrame) throw new Error('Load a reference screenshot or capture a frame first');
  if (cropEditor.active || profileChanging) throw new Error('Another editor operation is active');
  const wasPaused = paused; generation++; const current = generation;
  if (running) paused = true;
  clearTimeout(timer); speaker.cancel();
  const bitmap = await createImageBitmap(latestFrame);
  try {
    return await cropEditor.open({ image: bitmap, width: bitmap.width, height: bitmap.height, initial,
      aspect: null, minimum: { width: 1, height: 1 }, purpose: 'region',
      description: 'Select the UI element or text field on this analyzed/reference frame. This edits the draft crop stage; apply the profile afterward.',
      applyLabel: 'Use this region' });
  } finally {
    bitmap.close();
    if (current === generation && running && capture.active) { paused = wasPaused; status(paused ? 'Coaching paused' : 'Coaching live'); if (!paused) schedule(0); }
  }
}

async function runReference() {
  if (!latestFrame) throw new Error('Load a reference screenshot or capture a frame first');
  if (pending || profileChanging || replaying || cropEditor.active) throw new Error('Wait for the current analysis/editor operation to finish');
  if (running) { generation++; paused = true; clearTimeout(timer); speaker.cancel(); status('Coaching paused', 'Running a reference frame.'); }
  await initialize();
  const rgba = new ImageData(new Uint8ClampedArray(latestFrame.data), latestFrame.width, latestFrame.height);
  viewport({ x: 0, y: 0, w: 1, h: 1 }, { width: rgba.width, height: rgba.height }, activeProfile.viewport.aspect_ratio);
  referenceTimestamp += 5000;
  const result = await analyzePixels({ rgba, map: new ImageData(1, 1), timestamp: referenceTimestamp, preview: null });
  apply(result.analysis, result.elapsed);
}

button('start').onclick = () => { void start(); };
for (const key of ['edit-viewport', 'fix-viewport']) button(key).onclick = () => { void editLiveViewport(); };
button('stop').onclick = () => { cancelMode(); status('Capture stopped', 'No further frames are being captured.'); };
button('pause').onclick = async () => {
  if (!running) return;
  paused = !paused; generation++; speaker.cancel();
  if (!paused && !ready) { try { await initialize(); } catch { paused = true; } }
  status(paused ? 'Coaching paused' : 'Coaching live'); if (!paused) schedule(0);
};
capture.onEnded = () => { cancelMode(); notice('Screen sharing ended. Choose Start coaching to share again.'); };
button('mute').onclick = () => { settings.muted = !settings.muted; speaker.configure(settings); saveSettings(settings); populateSettings(); };
speaker.onStatus = s => text('voice-state', s);
button('test-voice').onclick = () => speaker.say({ key: 'test', text: 'Coach audio test. Your next move starts here.', priority: 'High', cooldown_secs: 0 }, true);
button('clear-cues').onclick = () => { cues = []; renderCues(); };
input('overlay').onchange = drawPreview;
button('demo').onclick = () => { void demo(); };
button('demo-replay').onclick = () => { void demo(); };
button('replay-import').onclick = () => { void importFiles(); };
button('cancel-replay').onclick = () => { cancelMode(); text('replay-status', 'Replay canceled.'); };
button('refresh-recordings').onclick = () => { void recordings(); };
button('export-snapshots').onclick = async () => {
  const f = await recordings(); if (!f.length) { notice('No recorded frames to export.'); return; }
  if (!isDeadlock(activeProfile)) {
    const reports = f.map(x => x.perception).filter(Boolean);
    if (!reports.length) { notice('No perception observations were recorded.'); return; }
    download('perception-recordings.jsonl', new Blob([reports.map(r => JSON.stringify(r)).join('\n') + '\n'], { type: 'application/x-ndjson' })); return;
  }
  download('deadlock-snapshots.jsonl', new Blob([f.map(x => JSON.stringify(x.snapshot)).join('\n') + '\n'], { type: 'application/x-ndjson' }));
};
button('export-recording').onclick = async () => {
  try {
    const frames = await recordings(); if (!frames.length) { notice('No recorded frames to export.'); return; }
    const rows = []; for (const f of frames) rows.push({ timestamp: f.timestamp, snapshot: f.snapshot, cues: f.cues, perception: f.perception, profile_id: f.profile_id, image: await blobDataURL(f.image) });
    download('deadlock-recording.json', new Blob([JSON.stringify({ schema: 1, assetVersion, frames: rows })], { type: 'application/json' }));
  } catch (e) { notice(`Export failed: ${String(e)}`, true); }
};
button('clear-recordings').onclick = async () => { try { await clearFrames(); await recordings(); notice('Local recordings deleted.'); } catch (e) { notice(String(e), true); } };
input('knowledge-search').oninput = renderKnowledge; $<HTMLSelectElement>('knowledge-category').onchange = renderKnowledge;
$<HTMLFormElement>('settings-form').onsubmit = e => { e.preventDefault(); saveForm(); };
input('rate').oninput = () => text('rate-value', `${Number(input('rate').value).toFixed(1)}×`);
input('volume').oninput = () => text('volume-value', `${Math.round(Number(input('volume').value) * 100)}%`);
button('reset-settings').onclick = () => { settings = structuredClone(defaults); saveSettings(settings); speaker.configure(settings); populateSettings(); worker?.postMessage({ type: 'configure', options: settings.options }); notice('Defaults restored.'); };
button('reload-engine').onclick = async () => {
  cancelMode(); ready = false; initializing = undefined; worker?.terminate(); worker = undefined;
  if (pending) { clearTimeout(pending.timer); pending.reject(new Error('Engine restarted')); pending = undefined; }
  try { await initialize(); notice('Analysis engine restarted.'); } catch (e) { notice(String(e), true); }
};
button('clear-cache').onclick = async () => { try { for (const key of await caches.keys()) if (key.startsWith('deadlock-assets-')) await caches.delete(key); notice('Model cache cleared. The next initialization will download models again.'); } catch (e) { notice(String(e), true); } };
window.addEventListener('hashchange', () => { route(); $('main').focus(); });
window.addEventListener('beforeunload', () => { capture.stop(); speaker.cancel(); });
window.speechSynthesis?.addEventListener('voiceschanged', voices);
// Detect browser scheduling stalls. Never synthesize stale cues while the game has moved on.
setInterval(() => {
  if (running && !paused && lastResultAt && Date.now() - lastResultAt > Math.max(30000, settings.interval * 4000)) {
    speaker.cancel(); text('session-state', 'Analysis delayed — check capture');
  }
}, 5000);
// Keep the existing percentage inputs as an advanced option; the visual editor is primary.
const calibration = input('crop-x').closest('fieldset')!;
const advanced = document.createElement('details'), advancedTitle = document.createElement('summary');
advancedTitle.textContent = 'Advanced percentage controls'; advanced.append(advancedTitle);
advanced.append(...Array.from(calibration.children).filter(el => el.tagName !== 'LEGEND'));
calibration.append(advanced);
calibration.querySelector('legend')!.after(($<HTMLTemplateElement>('visual-calibration-controls').content.cloneNode(true)));
for (const key of ['x', 'y', 'w', 'h']) { input(`crop-${key}`).min = key === 'x' || key === 'y' ? '0' : '0.01'; input(`crop-${key}`).max = '100'; input(`crop-${key}`).step = 'any'; }
// Settings editor controls were cloned after event setup.
button('edit-viewport-settings').onclick = () => { void editLiveViewport(); };
profilePanel = new ProfilePanel({ current: () => activeProfile, apply: applyGameProfile, pick: pickFieldRegion,
  reference: loadReferenceImage, runReference });
refreshGameLayout();
populateSettings(); renderSnapshot(latest); diagnostics(); route(); void loadNames();
if (import.meta.env.PROD && 'serviceWorker' in navigator) {
  void navigator.serviceWorker.register(`${base}sw.js`, { scope: new URL(import.meta.env.BASE_URL, location.origin).pathname })
    .then(reg => {
      reg.addEventListener('updatefound', () => {
        reg.installing?.addEventListener('statechange', () => {
          if (reg.waiting && navigator.serviceWorker.controller) notice('An app update is cached. Close other Coach tabs and reload after your match to apply it.');
        });
      });
    }).catch(() => { /* Offline installation is optional. Analysis works without a service worker. */ });
}
