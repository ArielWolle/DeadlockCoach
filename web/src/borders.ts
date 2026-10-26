import { ASPECT, normalized, viewport, type Crop, type Rect, type Size } from './crop.ts';

interface Pixels { width: number; height: number; data: ArrayLike<number> }

/** Conservative edge-only letterbox/pillarbox detection. A dark or ambiguous frame is
 * not evidence of borders: require a visible content area and an approximately 16:9 result. */
export function detectBorders(pixels: Pixels, source: Size, aspect: number | null = ASPECT): Rect | undefined {
  const { width: w, height: h, data } = pixels;
  if (w < 32 || h < 18 || data.length !== w * h * 4 || !source.width || !source.height) return;
  const black = (x: number, y: number) => {
    const i = (y * w + x) * 4;
    return Math.max(data[i], data[i + 1], data[i + 2]) <= 26;
  };
  // Sparse player controls/channel badges may cover an otherwise black bar.
  const darkRow = (y: number, left: number, right: number) => {
    let light = 0; for (let x = left; x < right; x++) if (!black(x, y)) light++;
    return light / (right - left) <= .065;
  };
  const darkColumn = (x: number, top: number, bottom: number) => {
    let light = 0; for (let y = top; y < bottom; y++) if (!black(x, y)) light++;
    return light / (bottom - top) <= .065;
  };
  let top = 0, bottom = h, left = 0, right = w;
  const limitY = Math.floor(h * .4), limitX = Math.floor(w * .4);
  while (top < limitY && darkRow(top, 0, w)) top++;
  while (h - bottom < limitY && darkRow(bottom - 1, 0, w)) bottom--;
  if (top === limitY || h - bottom === limitY) return;
  while (left < limitX && darkColumn(left, top, bottom)) left++;
  while (w - right < limitX && darkColumn(right - 1, top, bottom)) right--;
  if (left === limitX || w - right === limitX) return;
  // Refine rows after removing side bars, which could have diluted their content ratio.
  while (top < limitY && darkRow(top, left, right)) top++;
  while (h - bottom < limitY && darkRow(bottom - 1, left, right)) bottom--;
  if (top === limitY || h - bottom === limitY) return;
  if (Math.max(top, h - bottom, left, w - right) < 2) return;
  let visible = 0;
  for (let y = top; y < bottom; y++) for (let x = left; x < right; x++) if (!black(x, y)) visible++;
  if (visible / ((right - left) * (bottom - top)) < .12) return;
  const x = left * source.width / w, y = top * source.height / h;
  const contentW = (right - left) * source.width / w, contentH = (bottom - top) * source.height / h;
  if (aspect !== null && Math.abs(contentW / contentH - aspect) > .04) return;
  if (aspect === null) {
    const rect = { x, y, w: contentW, h: contentH };
    try { viewport(normalized(rect, source), source, null); return rect; } catch { return; }
  }
  // Snap to exact 16:9 while including the detected content rather than clipping HUD edges.
  let width = Math.max(contentW, contentH * aspect), height = width / aspect;
  const scale = Math.min(1, source.width / width, source.height / height); width *= scale; height *= scale;
  const rect = { x: Math.max(0, Math.min(source.width - width, x + contentW / 2 - width / 2)),
    y: Math.max(0, Math.min(source.height - height, y + contentH / 2 - height / 2)), w: width, h: height };
  try { viewport(normalized(rect, source), source, aspect); return rect; } catch { return; }
}

export function consistentBorders(rects: Array<Rect | undefined>, size: Size): Crop | undefined {
  if (rects.length < 3 || rects.some(r => !r)) return;
  const valid = rects as Rect[];
  for (const key of ['x', 'y', 'w', 'h'] as const) {
    const values = valid.map(r => r[key]), tolerance = Math.max(3, (key === 'x' || key === 'w' ? size.width : size.height) * .006);
    if (Math.max(...values) - Math.min(...values) > tolerance) return;
  }
  const average = Object.fromEntries(['x', 'y', 'w', 'h'].map(key => [key, valid.reduce((sum, r) => sum + r[key as keyof Rect], 0) / valid.length])) as unknown as Rect;
  return normalized(average, size);
}

/** Sample a live source over three frames; never rely on one dark gameplay frame. */
export async function sampleBorders(image: HTMLVideoElement | ImageBitmap, size: Size, aspect: number | null = ASPECT): Promise<Crop | undefined> {
  const canvas = document.createElement('canvas');
  const scale = Math.min(1, 640 / size.width, 480 / size.height);
  canvas.width = Math.round(size.width * scale); canvas.height = Math.round(size.height * scale);
  const ctx = canvas.getContext('2d', { willReadFrequently: true })!;
  const frames: Array<Rect | undefined> = [];
  for (let i = 0; i < 3; i++) {
    if (image instanceof HTMLVideoElement && (image.readyState < 2 || image.videoWidth !== size.width || image.videoHeight !== size.height)) return;
    ctx.drawImage(image, 0, 0, canvas.width, canvas.height);
    frames.push(detectBorders(ctx.getImageData(0, 0, canvas.width, canvas.height), size, aspect));
    if (!frames[i]) return;
    if (i < 2 && image instanceof HTMLVideoElement) await new Promise(resolve => setTimeout(resolve, 150));
  }
  return consistentBorders(frames, size);
}
