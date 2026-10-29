import type { Settings } from './types';
export const defaults: Settings = {
  muted: false, voice: '', rate: 1, volume: 1, interval: 1, record: false, autoCrop: true,
  options: { timers: true, economy: true, map: true, counters: true, deaths: true, laning:true },
  laneFocus:'auto',
  heroFocus:'',
  playerProfile:true,
  crop: { x: 0, y: 0, w: 1, h: 1 },
  // Optional AI coach: off by default.
  aiEnabled: false, aiProvider: 'openai', aiBaseUrl: '', aiModel: '', aiKey: '', aiInterval: 40, aiMaxTokens: 1024, aiSpeak: true, aiHeaders: '',
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
  // AI coach fields (top level, never part of `options`, which is sent to the engine).
  s.aiEnabled = value.aiEnabled === true; s.aiSpeak = value.aiSpeak !== false;
  s.aiProvider = value.aiProvider === 'anthropic' ? 'anthropic' : 'openai';
  s.aiBaseUrl = typeof value.aiBaseUrl === 'string' ? value.aiBaseUrl.trim().slice(0, 300) : '';
  s.aiModel = typeof value.aiModel === 'string' ? value.aiModel.trim().slice(0, 120) : '';
  s.aiKey = typeof value.aiKey === 'string' ? value.aiKey.trim().slice(0, 500) : '';
  s.aiHeaders = typeof value.aiHeaders === 'string' ? value.aiHeaders.slice(0, 2000) : '';
  s.aiInterval = Math.round(number(value.aiInterval, 15, 600, defaults.aiInterval));
  // 200 was the old default and cut replies off (OpenRouter + Claude Haiku stopped mid-JSON): saved
  // values below 512 are raised to the new default. Still editable in Settings (512-8192).
  const tokens = Math.round(number(value.aiMaxTokens, 32, 8192, defaults.aiMaxTokens));
  s.aiMaxTokens = tokens < 512 ? defaults.aiMaxTokens : tokens;
  s.playerProfile=value.playerProfile!==false;
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
