import type { Settings } from './types';

export type Crop = Settings['crop'];
export interface Size { width: number; height: number }
export interface Rect { x: number; y: number; w: number; h: number }
export type Corner = 'nw' | 'ne' | 'sw' | 'se';
export const ASPECT = 16 / 9;
const clamp = (n: number, lo: number, hi: number) => Math.max(lo, Math.min(hi, n));

export class ViewportError extends Error {
  constructor(message: string) { super(message); this.name = 'ViewportError'; }
}

export function viewport(crop: Crop, size: Size): Rect {
  const { width, height } = size;
  const x = Math.round(width * crop.x), y = Math.round(height * crop.y);
  const w = Math.min(width - x, Math.round(width * crop.w));
  const h = Math.min(height - y, Math.round(height * crop.h));
  if (![x, y, w, h].every(Number.isFinite) || x < 0 || y < 0 || w < 320 || h < 180) {
    throw new ViewportError('The selected game area is too small. Use Adjust viewport to select at least 320×180 pixels.');
  }
  if (Math.abs(w / h - ASPECT) > .06) {
    throw new ViewportError('Select the 16:9 game area using Adjust viewport. Exclude browser borders, chat and black bars.');
  }
  return { x, y, w, h };
}

export function normalized(rect: Rect, size: Size): Crop {
  return { x: rect.x / size.width, y: rect.y / size.height, w: rect.w / size.width, h: rect.h / size.height };
}

export function fit(size: Size): Rect {
  const w = Math.min(size.width, size.height * ASPECT), h = w / ASPECT;
  return { x: (size.width - w) / 2, y: (size.height - h) / 2, w, h };
}

export function selection(crop: Crop, size: Size): Rect {
  const max = fit(size);
  const w = clamp(Math.min(crop.w * size.width, crop.h * size.height * ASPECT), Math.min(320, max.w), max.w);
  const h = w / ASPECT;
  return { x: clamp((crop.x + crop.w / 2) * size.width - w / 2, 0, size.width - w),
    y: clamp((crop.y + crop.h / 2) * size.height - h / 2, 0, size.height - h), w, h };
}

export function move(rect: Rect, dx: number, dy: number, size: Size): Rect {
  return { ...rect, x: clamp(rect.x + dx, 0, size.width - rect.w), y: clamp(rect.y + dy, 0, size.height - rect.h) };
}

export function resize(rect: Rect, corner: Corner, dx: number, dy: number, size: Size): Rect {
  const sx = corner.endsWith('e') ? 1 : -1, sy = corner.startsWith('s') ? 1 : -1;
  const ax = sx > 0 ? rect.x : rect.x + rect.w, ay = sy > 0 ? rect.y : rect.y + rect.h;
  const max = Math.min(sx > 0 ? size.width - ax : ax, (sy > 0 ? size.height - ay : ay) * ASPECT);
  const delta = Math.abs(dx) >= Math.abs(dy) * ASPECT ? sx * dx : sy * dy * ASPECT;
  const w = clamp(rect.w + delta, Math.min(320, max), max), h = w / ASPECT;
  return { x: sx > 0 ? ax : ax - w, y: sy > 0 ? ay : ay - h, w, h };
}

export function grow(rect: Rect, delta: number, size: Size): Rect {
  const cx = rect.x + rect.w / 2, cy = rect.y + rect.h / 2;
  const max = Math.min(cx * 2, (size.width - cx) * 2, cy * 2 * ASPECT, (size.height - cy) * 2 * ASPECT);
  const w = clamp(rect.w + delta, Math.min(320, max), max), h = w / ASPECT;
  return { x: cx - w / 2, y: cy - h / 2, w, h };
}
