import type { Settings } from './types';
export const defaults: Settings = {
  muted: false, voice: '', rate: 1, volume: 1, interval: 1, record: false, autoCrop: true,
  options: { timers: true, economy: true, map: true, counters: true, deaths: true, laning:true },
  laneFocus:'auto',
  heroFocus:'',
  crop: { x: 0, y: 0, w: 1, h: 1 },
};
const number = (n: unknown, lo: number, hi: number, fallback: number) => typeof n === 'number' && Number.isFinite(n) ? Math.min(hi, Math.max(lo, n)) : fallback;
export function normalize(input: unknown): Settings {
  const value = input && typeof input === 'object' ? input as Partial<Settings> : {};
  const s = structuredClone(defaults);
  s.muted = value.muted === true; s.record = value.record === true;
  s.autoCrop = value.autoCrop !== false;
  if (['auto','Green','Blue','Yellow'].includes(value.laneFocus || '')) s.laneFocus=value.laneFocus!;
  s.voice = typeof value.voice === 'string' ? value.voice.slice(0, 200) : '';
  s.rate = number(value.rate, .5, 2, defaults.rate); s.volume = number(value.volume, 0, 1, defaults.volume);
  s.interval = number(value.interval, 1, 30, defaults.interval);
  s.heroFocus=typeof value.heroFocus==='string'?value.heroFocus.slice(0,80):'';
  for (const key of Object.keys(s.options) as Array<keyof Settings['options']>) if (typeof value.options?.[key] === 'boolean') s.options[key] = value.options[key];
  const c = value.crop;
  // Preserve precise visual selections, including a small game player inside a large
  // shared display. Pixel-size validation belongs to the capture/editor source geometry.
  s.crop.x = number(c?.x, 0, .9999, 0); s.crop.y = number(c?.y, 0, .9999, 0);
  s.crop.w = number(c?.w, .0001, 1 - s.crop.x, 1 - s.crop.x); s.crop.h = number(c?.h, .0001, 1 - s.crop.y, 1 - s.crop.y);
  return s;
}
export function loadSettings(): Settings {
  try { return normalize(JSON.parse(localStorage.getItem('deadlock-coach-settings-v1') || '{}')); } catch { return structuredClone(defaults); }
}
export function saveSettings(s: Settings) {
  try { localStorage.setItem('deadlock-coach-settings-v1', JSON.stringify(normalize(s))); } catch { /* private mode/quota: keep in memory */ }
}
