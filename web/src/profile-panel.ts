import { builtInProfiles, defaultProfile, parseProfile, type GameProfile, type ProfileNode } from './profiles';
import type { Crop } from './crop';
import type { PerceptionReport, Reading } from './types';
import { download } from './storage';

const el = <T extends HTMLElement = HTMLElement>(id: string) => document.getElementById(id) as T;
const short = (value: unknown, max = 240) => { const s = typeof value === 'string' ? value : JSON.stringify(value) ?? '—'; return s.length > max ? `${s.slice(0, max)}…` : s; };
interface Callbacks {
  current: () => GameProfile;
  apply: (profile: GameProfile) => Promise<void>;
  pick: (initial: Crop) => Promise<Crop | undefined>;
  reference: (file: File) => Promise<void>;
  runReference: () => Promise<void>;
}

export class ProfilePanel {
  private report?: PerceptionReport;
  private records: string[] = [];
  private bytes = 0;
  constructor(private callbacks: Callbacks) {
    const presets = el<HTMLSelectElement>('profile-preset');
    for (const p of builtInProfiles) presets.add(new Option(p.name, p.id));
    this.draft(callbacks.current()); this.updateActive();
    el('profile-load-preset').onclick = () => {
      const p = builtInProfiles.find(p => p.id === presets.value); if (p) { this.draft(structuredClone(p)); this.status('Starter loaded into the draft. Apply to activate it.'); }
    };
    el('profile-apply').onclick = () => { void this.apply(); };
    el('profile-restore').onclick = () => { this.draft(defaultProfile()); void this.apply(); };
    el<HTMLTextAreaElement>('profile-json').onchange = () => { try { this.regions(this.read()); } catch { /* keep the draft editable */ } };
    el<HTMLInputElement>('profile-import').onchange = async e => {
      try {
        const file = (e.target as HTMLInputElement).files?.[0]; if (!file) return;
        if (file.size > 4 * 1024 * 1024) throw new Error('Profile exceeds 4 MB');
        this.draft(parseProfile(await file.text())); this.status('Imported draft. Apply validates the graph before activating it.');
      } catch (e) { this.status(String(e), true); }
    };
    el('profile-export').onclick = () => {
      try { const p = this.read(); download(`${p.id.replace(/[^a-zA-Z0-9_-]/g, '_')}.profile.json`, new Blob([JSON.stringify(p, null, 2)], { type: 'application/json' })); }
      catch (e) { this.status(String(e), true); }
    };
    el('profile-add-field').onclick = () => { try { this.addField(); } catch (e) { this.status(String(e), true); } };
    el('profile-pick-region').onclick = async () => {
      try {
        const p = this.read(), id = el<HTMLSelectElement>('profile-region-node').value;
        const node = p.nodes.find(n => n.id === id); if (!node) throw new Error('Choose a crop stage first');
        const r = node.op.region as { x: number; y: number; w: number; h: number; space?: string; anchor?: string };
        if (r.space && r.space !== 'normalized' || r.anchor && r.anchor !== 'top_left' || node.op.anchor_input) throw new Error('Visual editing supports normalized top-left crops. Use JSON for anchored/reference/pixel regions.');
        const crop = await callbacks.pick({ x: r.x, y: r.y, w: r.w, h: r.h });
        if (crop) { node.op.region = crop; this.draft(p, id); this.status(`Updated ${id} in the draft. Apply profile to activate the region.`); }
      } catch (e) { this.status(String(e), true); }
    };
    el<HTMLInputElement>('profile-reference').onchange = async e => {
      const file = (e.target as HTMLInputElement).files?.[0]; if (!file) return;
      try { await callbacks.reference(file); this.status('Reference image loaded. Select a region or run the active pipeline.'); }
      catch (e) { this.status(String(e), true); }
    };
    el('profile-run-reference').onclick = async () => {
      const b = el<HTMLButtonElement>('profile-run-reference'); b.disabled = true;
      this.status('Running the active pipeline on the reference frame…');
      try { await callbacks.runReference(); this.status('Reference frame processed by the active pipeline.'); }
      catch (e) { this.status(String(e), true); } finally { b.disabled = false; }
    };
    el('profile-add-template').onclick = async () => {
      const button = el<HTMLButtonElement>('profile-add-template'); button.disabled = true;
      this.status('Preparing template image…');
      try { await this.addTemplate(); } finally { button.disabled = false; }
    };
    el('profile-export-report').onclick = () => {
      if (!this.report) { this.status('Analyze a frame first.'); return; }
      download('perception-report.json', new Blob([JSON.stringify(this.report, null, 2)], { type: 'application/json' }));
    };
    el('profile-export-jsonl').onclick = () => {
      if (!this.records.length) { this.status('No observations to export yet.'); return; }
      download('perception-observations.jsonl', new Blob([this.records.join('\n') + '\n'], { type: 'application/x-ndjson' }));
    };
    this.clear();
  }
  private read() { return parseProfile(el<HTMLTextAreaElement>('profile-json').value); }
  private draft(p: GameProfile, regionId?: string) { el<HTMLTextAreaElement>('profile-json').value = JSON.stringify(p, null, 2); this.regions(p, regionId); }
  private regions(p: GameProfile, selected = el<HTMLSelectElement>('profile-region-node').value) {
    const select = el<HTMLSelectElement>('profile-region-node'); select.replaceChildren();
    for (const n of p.nodes.filter(n => n.op.type === 'crop')) select.add(new Option(n.id, n.id));
    if (selected && [...select.options].some(o => o.value === selected)) select.value = selected;
  }
  private status(message: string, error = false) { el('profile-status').textContent = message; el('profile-status').classList.toggle('profile-error', error); }
  updateActive() { const p = this.callbacks.current(); el('profile-active').textContent = `Active: ${p.name} · ${p.adapter === 'deadlock' ? 'Deadlock coaching adapter' : 'Generic perception only'} · ${p.nodes.length} stages`; }
  private async apply() {
    const button = el<HTMLButtonElement>('profile-apply'); button.disabled = true; this.status('Validating and preparing profile resources…');
    try { const p = this.read(); await this.callbacks.apply(p); this.updateActive(); this.status('Profile validated and active. Its outputs are published independently of game-specific coaching.'); }
    catch (e) { this.status(`Profile not applied: ${e instanceof Error ? e.message : String(e)}`, true); }
    finally { button.disabled = false; }
  }
  private id(p: GameProfile, name: string) { let id = name.replace(/[^a-zA-Z0-9_]/g, '_').slice(0, 60) || 'field'; const base = id; let i = 1; while (p.nodes.some(n => n.id.startsWith(id + '_') || n.id === id)) id = `${base}_${i++}`; return id; }
  private addField() {
    const p = this.read(), name = el<HTMLInputElement>('profile-field-name').value.trim(); if (!name) throw new Error('Enter a field name');
    if (p.outputs[name]) throw new Error('That output already exists');
    const id = this.id(p, name), kind = el<HTMLSelectElement>('profile-field-kind').value;
    const crop: ProfileNode = { id: `${id}_region`, op: { type: 'crop', region: { x: .4, y: .4, w: .2, h: .1 } } };
    const read: ProfileNode = { id: `${id}_read`, inputs: [crop.id], op: kind === 'color'
      ? { type: 'color_ratio', rgb: [60, 200, 80], tolerance: 45 }
      : { type: 'ocr', mode: 'detect', upscale: 3 } };
    const parse: ProfileNode = { id: `${id}_parsed`, inputs: [read.id], op: { type: 'parse', parser: kind === 'color' ? 'number' : kind, scale: kind === 'color' ? 100 : 1 } };
    const stable: ProfileNode = { id: `${id}_stable`, inputs: [parse.id], op: { type: 'confirm', frames: 2, tolerance: kind === 'text' ? 0 : 10, max_age_ms: 20000 } };
    p.nodes.push(crop, read, parse, stable); p.outputs[name] = stable.id;
    this.draft(p, crop.id); this.status('Field added. Select its region, then tune parsing/confirmation settings as needed.');
  }
  private async addTemplate() {
    try {
      const p = this.read(), file = el<HTMLInputElement>('profile-template-file').files?.[0], id = el<HTMLInputElement>('profile-template-id').value.trim();
      if (!file || !id) throw new Error('Choose a template image and enter its ID');
      if (file.size > 4 * 1024 * 1024) throw new Error('Choose an image smaller than 4 MB');
      if ((p.templates || []).some(t => t.id === id)) throw new Error('That template ID already exists');
      const bitmap = await createImageBitmap(file), canvas = document.createElement('canvas');
      const scale = Math.min(1, 256 / bitmap.width, 256 / bitmap.height);
      canvas.width = Math.max(1, Math.round(bitmap.width * scale)); canvas.height = Math.max(1, Math.round(bitmap.height * scale));
      canvas.getContext('2d')!.drawImage(bitmap, 0, 0, canvas.width, canvas.height); bitmap.close();
      p.templates ||= []; p.templates.push({ id, png_base64: canvas.toDataURL('image/png').split(',')[1] });
      const stageId = this.id(p, id);
      p.nodes.push({ id: stageId, op: { type: 'locate', template: id, scales: [1], step: 16, min_score: .8 } });
      p.outputs[`ui.${id}`] = stageId;
      this.draft(p); this.status('Template and search stage added. Crop the search input or adjust scale/step settings for your game.');
    } catch (e) { this.status(String(e), true); }
  }

  clear() { this.report = undefined; this.records = []; this.bytes = 0; this.readings({}); el('profile-traces').replaceChildren(); }
  render(report: PerceptionReport) {
    this.report = report;
    const line = JSON.stringify({ schema_version: report.schema_version, profile_id: report.profile_id,
      profile_revision: report.profile_revision, frame_id: report.frame_id, timestamp_ms: report.timestamp_ms, frame_size: report.frame_size, fields: report.fields });
    this.records.push(line); this.bytes += line.length;
    while (this.records.length > 300 || this.bytes > 4 * 1024 * 1024) this.bytes -= this.records.shift()!.length;
    this.readings(report.fields);
    const body = el('profile-traces'); body.replaceChildren();
    for (const s of report.stages) {
      const row = document.createElement('tr');
      const values = [s.id, `${s.operation} ← ${s.inputs.join(', ')}`, s.status,
        s.error || (s.value == null && s.image_size ? `Image ${s.image_size.join('×')}` : short(s.value)),
        s.score == null ? 'Unknown' : s.score.toFixed(3), `${s.elapsed_ms.toFixed(1)} ms`];
      for (const value of values) { const td = document.createElement('td'); td.textContent = value; row.append(td); } body.append(row);
    }
  }
  private readings(fields: Record<string, Reading>) {
    for (const id of ['profile-readings', 'generic-readings']) {
      const root = el(id); root.replaceChildren();
      if (!Object.keys(fields).length) { const p = document.createElement('p'); p.className = 'muted'; p.textContent = 'Analyze a frame to inspect published fields.'; root.append(p); continue; }
      const list = document.createElement('dl'); list.className = 'profile-field-list';
      for (const [key, r] of Object.entries(fields)) {
        const name = document.createElement('dt'), value = document.createElement('dd'); name.textContent = key;
        value.textContent = `${r.status.toUpperCase()} · ${r.status === 'ok' ? short(r.value) : 'No confirmed value'}`; list.append(name, value);
      } root.append(list);
    }
  }
}
