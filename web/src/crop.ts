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

export function viewport(crop: Crop, size: Size, aspect: number | null = ASPECT, minWidth = 320, minHeight = 180): Rect {
  const { width, height } = size;
  const x = Math.round(width * crop.x), y = Math.round(height * crop.y);
  const w = Math.min(width - x, Math.round(width * crop.w));
  const h = Math.min(height - y, Math.round(height * crop.h));
  if (![x, y, w, h].every(Number.isFinite) || x < 0 || y < 0 || w < minWidth || h < minHeight) {
    throw new ViewportError(`The selected area is too small. Select at least ${minWidth}×${minHeight} pixels.`);
  }
  if (aspect !== null && Math.abs(w / h - aspect) > .06) {
    throw new ViewportError('Select the game area matching this profile using Adjust viewport. Exclude browser borders, chat and black bars.');
  }
  return { x, y, w, h };
}

export function normalized(rect: Rect, size: Size): Crop {
  return { x: rect.x / size.width, y: rect.y / size.height, w: rect.w / size.width, h: rect.h / size.height };
}

export function fit(size: Size, aspect: number | null = ASPECT): Rect {
  if (aspect === null) return { x: 0, y: 0, w: size.width, h: size.height };
  const w = Math.min(size.width, size.height * aspect), h = w / aspect;
  return { x: (size.width - w) / 2, y: (size.height - h) / 2, w, h };
}

export function selection(crop: Crop, size: Size, aspect: number | null = ASPECT, minWidth = 320, minHeight = 180): Rect {
  const max = fit(size, aspect);
  const w = aspect === null ? clamp(crop.w * size.width, Math.min(minWidth, size.width), size.width)
    : clamp(Math.min(crop.w * size.width, crop.h * size.height * aspect), Math.min(Math.max(minWidth, minHeight * aspect), max.w), max.w);
  const h = aspect === null ? clamp(crop.h * size.height, Math.min(minHeight, size.height), size.height) : w / aspect;
  return { x: clamp((crop.x + crop.w / 2) * size.width - w / 2, 0, size.width - w),
    y: clamp((crop.y + crop.h / 2) * size.height - h / 2, 0, size.height - h), w, h };
}

export function move(rect: Rect, dx: number, dy: number, size: Size): Rect {
  return { ...rect, x: clamp(rect.x + dx, 0, size.width - rect.w), y: clamp(rect.y + dy, 0, size.height - rect.h) };
}

export function resize(rect: Rect, corner: Corner, dx: number, dy: number, size: Size, aspect: number | null = ASPECT, minWidth = 320, minHeight = 180): Rect {
  const sx = corner.endsWith('e') ? 1 : -1, sy = corner.startsWith('s') ? 1 : -1;
  const ax = sx > 0 ? rect.x : rect.x + rect.w, ay = sy > 0 ? rect.y : rect.y + rect.h;
  const maxX = sx > 0 ? size.width - ax : ax, maxY = sy > 0 ? size.height - ay : ay;
  const max = aspect === null ? maxX : Math.min(maxX, maxY * aspect);
  const delta = aspect === null || Math.abs(dx) >= Math.abs(dy) * aspect ? sx * dx : sy * dy * aspect;
  const w = clamp(rect.w + delta, Math.min(aspect === null ? minWidth : Math.max(minWidth, minHeight * aspect), max), max);
  const h = aspect === null ? clamp(rect.h + sy * dy, Math.min(minHeight, maxY), maxY) : w / aspect;
  return { x: sx > 0 ? ax : ax - w, y: sy > 0 ? ay : ay - h, w, h };
}

export function grow(rect: Rect, delta: number, size: Size, aspect: number | null = ASPECT, minWidth = 320, minHeight = 180): Rect {
  const ratio = aspect ?? rect.w / rect.h;
  const cx = rect.x + rect.w / 2, cy = rect.y + rect.h / 2;
  const max = Math.min(cx * 2, (size.width - cx) * 2, cy * 2 * ratio, (size.height - cy) * 2 * ratio);
  const w = clamp(rect.w + delta, Math.min(Math.max(minWidth, minHeight * ratio), max), max), h = w / ratio;
  return { x: cx - w / 2, y: cy - h / 2, w, h };
}
