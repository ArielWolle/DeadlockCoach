import type { Settings } from './types';
import { viewport } from './crop';
import type { GameProfile } from './profiles';

export interface Pixels { rgba: ImageData; map: ImageData; timestamp: number; preview: Blob | null }
export class Capture {
  readonly video = document.createElement('video');
  private stream?: MediaStream;
  private canvas = document.createElement('canvas');
  private mapCanvas = document.createElement('canvas');
  private lastTime = -1;
  private advancedAt = performance.now();
  private aspect: number | null = 16 / 9;
  private maxWidth = 1600;
  private legacy = true;
  onEnded: () => void = () => {};
  configure(profile: GameProfile) { this.aspect = profile.viewport.aspect_ratio; this.maxWidth = profile.viewport.max_width; this.legacy = profile.adapter === 'deadlock'; }

  async start() {
    if (!navigator.mediaDevices?.getDisplayMedia) throw new Error('Screen sharing is unavailable. Open this site in desktop Chrome or Edge over HTTPS (or localhost).');
    // Must be called directly from a user gesture, BEFORE loading models or other awaits.
    const stream = await navigator.mediaDevices.getDisplayMedia({
      video: { displaySurface: 'window', frameRate: { ideal: 5, max: 10 } }, audio: false,
    });
    this.stop(); this.stream = stream;
    const track = stream.getVideoTracks()[0];
    track.addEventListener('ended', () => { this.stop(); this.onEnded(); }, { once: true });
    this.video.srcObject = stream; this.video.muted = true; this.video.playsInline = true;
    try { await this.video.play(); } catch (error) { this.stop(); throw error; }
    this.lastTime = -1; this.advancedAt = performance.now();
    return { label: track.label || 'Shared display', width: this.video.videoWidth, height: this.video.videoHeight };
  }

  get active() { return !!this.stream?.getVideoTracks().some(t => t.readyState === 'live'); }
  stop() {
    this.stream?.getTracks().forEach(t => t.stop()); this.stream = undefined;
    this.video.pause(); this.video.srcObject = null;
  }

  async frame(crop: Settings['crop'], record: boolean): Promise<Pixels> {
    if (!this.active || this.video.readyState < 2) throw new Error('Capture has no readable video frame. Check the shared window.');
    if (this.video.currentTime !== this.lastTime) { this.advancedAt = performance.now(); this.lastTime = this.video.currentTime; }
    if (performance.now() - this.advancedAt > 20000 || this.stream?.getVideoTracks()[0].muted) throw new Error('The shared video is stale or muted. Restore the game window or restart sharing.');
    return this.fromSource(this.video, this.video.videoWidth, this.video.videoHeight, crop, record);
  }

  async image(file: Blob, crop: Settings['crop']): Promise<Pixels> {
    const bitmap = await createImageBitmap(file);
    try { return await this.fromSource(bitmap, bitmap.width, bitmap.height, crop, false); }
    finally { bitmap.close(); }
  }

  private async fromSource(source: CanvasImageSource, width: number, height: number, crop: Settings['crop'], record: boolean): Promise<Pixels> {
    if (width < 320 || height < 180) throw new Error('Source is too small for HUD recognition (minimum 320×180).');
    const { x, y, w, h } = viewport(crop, { width, height }, this.aspect);
    const frameScale = Math.min(1, this.maxWidth / w, 2304 / h);
    if (!this.legacy && (Math.round(w * frameScale) < 320 || Math.round(h * frameScale) < 180)) throw new Error('Selected area is too narrow or tall for the current processing size. Select a larger game viewport.');
    this.canvas.width = this.legacy ? 1600 : Math.max(320, Math.round(w * frameScale));
    this.canvas.height = this.legacy ? 900 : Math.max(180, Math.round(h * frameScale));
    const ctx = this.canvas.getContext('2d', { willReadFrequently: true })!;
    ctx.drawImage(source, x, y, w, h, 0, 0, this.canvas.width, this.canvas.height);
    const mx = x + w * 1255 / 1600, my = y + h * 525 / 900;
    const mw = w * 335 / 1600, mh = h * 350 / 900;
    const scale = Math.min(1, 800 / mw);
    this.mapCanvas.width = this.legacy ? Math.round(mw * scale) : 1; this.mapCanvas.height = this.legacy ? Math.round(mh * scale) : 1;
    const mapCtx = this.mapCanvas.getContext('2d', { willReadFrequently: true })!;
    if (this.legacy) mapCtx.drawImage(source, mx, my, mw, mh, 0, 0, this.mapCanvas.width, this.mapCanvas.height);
    const preview = record ? await new Promise<Blob | null>(resolve => this.canvas.toBlob(resolve, 'image/jpeg', .8)) : null;
    return { rgba: ctx.getImageData(0, 0, this.canvas.width, this.canvas.height), map: mapCtx.getImageData(0, 0, this.mapCanvas.width, this.mapCanvas.height),
      timestamp: Date.now(), preview };
  }
}
