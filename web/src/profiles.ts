import deadlock from '../../data/profiles/deadlock.json';
import generic from '../../data/profiles/generic-hud.json';
import colorBar from '../../data/profiles/color-bar.json';
import deadlockFields from '../../data/profiles/deadlock-hud-fields.json';
import automatic from '../../data/profiles/deadlock-auto.json';

export interface ProfileNode { id: string; inputs?: string[]; when?: string; op: { type: string; [key: string]: unknown } }
export interface GameProfile {
  schema_version: number; id: string; name: string; adapter: 'generic' | 'deadlock';
  viewport: { aspect_ratio: number | null; max_width: number; auto_crop?:boolean };
  templates?: Array<{ id: string; png_base64: string }>;
  nodes: ProfileNode[]; outputs: Record<string, string>;
}
export const builtInProfiles = [automatic, deadlock, generic, colorBar, deadlockFields] as GameProfile[];
export const defaultProfile = () => structuredClone(builtInProfiles[0]);
export function parseProfile(text: string): GameProfile {
  if (text.length > 4 * 1024 * 1024) throw new Error('Profile exceeds 4 MB. Use smaller template images.');
  const p = JSON.parse(text.replace(/^\uFEFF/, ''));
  if (!p || p.schema_version !== 1 || typeof p.id !== 'string' || !p.id || typeof p.name !== 'string' || !p.name
    || !Array.isArray(p.nodes) || !p.nodes.length || p.nodes.length > 128 || !p.outputs || typeof p.outputs !== 'object' || Array.isArray(p.outputs)) {
    throw new Error('Profile needs schema_version 1, id, name, 1–128 nodes and an outputs object.');
  }
  p.adapter ??= 'generic'; p.viewport ??= { aspect_ratio: null, max_width: 1600 };
  p.viewport.aspect_ratio ??= null; p.viewport.max_width ??= 1600;
  if (!['generic', 'deadlock'].includes(p.adapter)) throw new Error('Unknown browser coaching adapter');
  const ratio = p.viewport.aspect_ratio;
  if ((ratio !== null && (typeof ratio !== 'number' || !Number.isFinite(ratio) || ratio < .2 || ratio > 8))
    || !Number.isInteger(p.viewport.max_width) || p.viewport.max_width < 320 || p.viewport.max_width > 4096) throw new Error('Invalid viewport aspect ratio or maximum width');
  return p as GameProfile; // Full node/resource/DAG validation is performed atomically in Rust.
}
export function loadProfile(): GameProfile {
  try { const saved = localStorage.getItem('coach-perception-profile-v1'); return saved ? parseProfile(saved) : defaultProfile(); }
  catch { return defaultProfile(); }
}
export function saveProfile(profile: GameProfile): boolean {
  try { localStorage.setItem('coach-perception-profile-v1', JSON.stringify(profile)); return true; } catch { return false; }
}
export const isDeadlock = (p: GameProfile) => p.adapter === 'deadlock';
