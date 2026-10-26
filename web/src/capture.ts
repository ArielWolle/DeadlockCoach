import type { Settings } from './types';
import { viewport } from './crop';
import type { GameProfile } from './profiles';
import {FrameHistory} from './frame-history';
import type {PerceptionReport} from './types';
import type {OverlayRegion} from './region-overlays';

export interface Pixels { rgba: ImageData; map: ImageData; timestamp: number; preview: Blob | null;sourceId?:number }
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
  private history=new FrameHistory();private frozen=false;private epoch=0;private inflight?:Promise<Pixels>;
  onEnded: () => void = () => {};
  configure(profile: GameProfile) { this.aspect = profile.viewport.aspect_ratio; this.maxWidth = profile.viewport.max_width; this.legacy = profile.nodes.some(n=>n.op.type==='custom'&&n.op.name==='deadlock.scan'); }

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
    this.epoch++;this.frozen=false;this.history.clear();
    this.stream?.getTracks().forEach(t => t.stop()); this.stream = undefined;
    this.video.pause(); this.video.srcObject = null;
  }

  async frame(crop: Settings['crop'], record: boolean): Promise<Pixels> {
    if(this.frozen)throw new Error('Frame acquisition is paused for inspection');
    if(this.inflight)throw new Error('A frame is already being acquired');
    if (!this.active || this.video.readyState < 2) throw new Error('Capture has no readable video frame. Check the shared window.');
    if (this.video.currentTime !== this.lastTime) { this.advancedAt = performance.now(); this.lastTime = this.video.currentTime; }
    if (performance.now() - this.advancedAt > 20000 || this.stream?.getVideoTracks()[0].muted) throw new Error('The shared video is stale or muted. Restore the game window or restart sharing.');
    const epoch=this.epoch;
    const task=(async()=>{const source=this.sourceCanvas();const timestamp=Date.now();const sourceId=await this.remember(source,crop,timestamp,epoch);
      const p=await this.fromSource(source,source.width,source.height,crop,record);p.timestamp=timestamp;p.sourceId=sourceId;return p;})();
    this.inflight=task;try{return await task;}finally{if(this.inflight===task)this.inflight=undefined;}
  }
  private sourceCanvas(){const canvas=document.createElement('canvas');canvas.width=this.video.videoWidth;canvas.height=this.video.videoHeight;canvas.getContext('2d')!.drawImage(this.video,0,0);return canvas;}
  private async remember(canvas:HTMLCanvasElement,crop:Settings['crop'],timestamp:number,epoch:number){
    const scale=Math.min(1,1920/canvas.width,1440/canvas.height);const width=Math.max(1,Math.round(canvas.width*scale)),height=Math.max(1,Math.round(canvas.height*scale));
    const image=await createImageBitmap(canvas,{resizeWidth:width,resizeHeight:height,resizeQuality:'high'});
    if(epoch!==this.epoch||!this.active){image.close();return undefined;}
    return this.history.add({image,width,height,rawWidth:canvas.width,rawHeight:canvas.height,timestamp,crop:{...crop}});
  }
  attachReport(id:number|undefined,report?:PerceptionReport|null){if(id!==undefined)this.history.attach(id,report);}
  attachRegions(id:number|undefined,regions:OverlayRegion[]){if(id!==undefined)this.history.attachRegions(id,regions);}
  async freeze(crop:Settings['crop']){
    this.frozen=true;this.video.pause();if(this.inflight)await this.inflight.catch(()=>{});
    if(!this.history.snapshot().length&&this.active&&this.video.readyState>=2)await this.remember(this.sourceCanvas(),crop,Date.now(),this.epoch);
    return this.history.snapshot();
  }
  async resume(){if(!this.active)return;await this.video.play();this.frozen=false;this.lastTime=-1;this.advancedAt=performance.now();}

  async image(file: Blob, crop: Settings['crop']): Promise<Pixels> {
    const bitmap = await createImageBitmap(file);
    try { return await this.fromSource(bitmap, bitmap.width, bitmap.height, crop, false); }
    finally { bitmap.close(); }
  }

  private async fromSource(source: CanvasImageSource, width: number, height: number, crop: Settings['crop'], record: boolean): Promise<Pixels> {
    if (!width || !height) throw new Error('Capture has no readable source pixels yet.');
    const { x, y, w, h } = viewport(crop, { width, height }, this.aspect,this.legacy?320:1,this.legacy?180:1);
    const frameScale = Math.min(1, this.maxWidth / w, 2304 / h);
    this.canvas.width = this.legacy ? 1600 : Math.max(1, Math.round(w * frameScale));
    this.canvas.height = this.legacy ? 900 : Math.max(1, Math.round(h * frameScale));
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
