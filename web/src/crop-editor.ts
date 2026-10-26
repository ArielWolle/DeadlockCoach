import { fit, grow, move, normalized, resize, selection, viewport, type Corner, type Crop, type Rect, type Size } from './crop';
import { sampleBorders } from './borders';
import type {SourceFrame} from './frame-history';
import {reportRegions,drawRegions,type OverlayRegion} from './region-overlays';
import {download} from './storage';

interface Source {
  image: HTMLVideoElement | ImageBitmap;
  width: number;
  height: number;
  initial: Crop;
  description: string;
  applyLabel: string;
  aspect?: number | null;
  minimum?: { width: number; height: number };
  purpose?: 'viewport' | 'region';
  frames?:SourceFrame[];
  regions?:OverlayRegion[];
}

/** Frozen capture-history/reference inspector; legacy video sources remain supported. */
export class CropEditor {
  private dialog = document.getElementById('crop-dialog') as HTMLDialogElement;
  private stage = document.getElementById('crop-stage')!;
  private box = document.getElementById('crop-selection')!;
  private canvas = document.getElementById('crop-source') as HTMLCanvasElement;
  private preview = document.getElementById('crop-preview') as HTMLCanvasElement;
  private source?: Source;
  private size: Size = { width: 1, height: 1 };
  private rect: Rect = { x: 0, y: 0, w: 1, h: 1 };
  private original: Crop = { x: 0, y: 0, w: 1, h: 1 };
  private resolve?: (crop: Crop | undefined) => void;
  private opening?: Promise<Crop | undefined>;
  private animation = 0;
  private lastPaint = -Infinity;
  private frameCount = 0;
  private aspect: number | null = 16 / 9;
  private minimum = { width: 320, height: 180 };
  private history:SourceFrame[]=[];private selected=-1;
  private drag?: { id: number; x: number; y: number; rect: Rect; corner?: Corner };

  constructor() {
    document.getElementById('crop-show-regions')!.onchange=()=>{this.renderSource();this.drawPreview();};
    document.getElementById('crop-export-sample')!.onclick=()=>{
      const frame=this.history[this.selected];if(!frame)return;
      const canvas=document.createElement('canvas');canvas.width=frame.width;canvas.height=frame.height;canvas.getContext('2d')!.drawImage(frame.image,0,0);
      const payload={schema_version:1,annotation_state:'unreviewed',timestamp_ms:frame.timestamp,image_base64:canvas.toDataURL('image/png').split(',')[1],
        profile_id:frame.report?.profile_id,profile_revision:frame.report?.profile_revision,
        frame_size:[frame.width,frame.height],original_source_size:[frame.rawWidth,frame.rawHeight],inspection_region:normalized(this.rect,this.size),capture_viewport:frame.crop,regions:reportRegions(frame)};
      download(`hud-inspection-${frame.id}.json`,new Blob([JSON.stringify(payload)],{type:'application/json'}));
    };
    this.dialog.addEventListener('cancel', () => this.cancel());
    this.dialog.addEventListener('close', () => { if (this.resolve && !this.dialog.open) this.finish(); });
    document.getElementById('crop-cancel')!.onclick = () => this.cancel();
    document.getElementById('crop-apply')!.onclick = () => {
      const crop = normalized(this.rect, this.size);
      try { viewport(crop, this.size, this.aspect, this.minimum.width, this.minimum.height); this.finish(crop); }
      catch (e) { this.announce(String(e instanceof Error ? e.message : e)); }
    };
    document.getElementById('crop-fit')!.onclick = () => { this.rect = fit(this.size, this.aspect); this.update(true); };
    document.getElementById('crop-reset')!.onclick = () => { this.rect = selection(this.original, this.size, this.aspect, this.minimum.width, this.minimum.height); this.update(true); };
    document.getElementById('crop-center')!.onclick = () => {
      this.rect.x = (this.size.width - this.rect.w) / 2; this.rect.y = (this.size.height - this.rect.h) / 2; this.update(true);
    };
    document.getElementById('crop-auto')!.onclick = async () => {
      const source = this.source; if (!source) return;
      const control = document.getElementById('crop-auto') as HTMLButtonElement;
      control.disabled = true; this.announce('Checking black borders across several frames…');
      try {
        const crop = await sampleBorders(source.image, { ...this.size }, this.aspect);
        if (source !== this.source) return;
        if (crop) { this.rect = selection(crop, this.size, this.aspect, this.minimum.width, this.minimum.height); this.update(); this.announce('Black borders detected. Review the cropped preview, then apply.'); }
        else this.announce('No reliable black borders detected. Drag the frame or use Fit 16:9 to select the game manually.');
      } catch { if (source === this.source) this.announce('Preview is temporarily unavailable. Try again or adjust manually.'); }
      finally { control.disabled = false; }
    };
    for (const [name, dx, dy] of [['left', -10, 0], ['right', 10, 0], ['up', 0, -10], ['down', 0, 10]] as const) {
      document.getElementById(`crop-${name}`)!.onclick = () => { this.rect = move(this.rect, dx, dy, this.size); this.update(true); };
    }
    for (const [name, delta] of [['smaller', -32], ['larger', 32]] as const) {
      document.getElementById(`crop-${name}`)!.onclick = () => { this.rect = grow(this.rect, delta, this.size, this.aspect, this.minimum.width, this.minimum.height); this.update(true); };
    }
    this.box.addEventListener('pointerdown', e => {
      if (e.button !== 0 || this.drag) return;
      const control = (e.target as HTMLElement).closest<HTMLElement>('[data-corner], #crop-move');
      if (!control) return;
      e.preventDefault(); control.focus();
      this.drag = { id: e.pointerId, x: e.clientX, y: e.clientY, rect: { ...this.rect }, corner: control.dataset.corner as Corner | undefined };
      this.box.setPointerCapture(e.pointerId);
    });
    this.box.addEventListener('pointermove', e => {
      const d = this.drag; if (!d || d.id !== e.pointerId) return;
      const bounds = this.stage.getBoundingClientRect();
      if (!bounds.width || !bounds.height) return;
      const dx = (e.clientX - d.x) * this.size.width / bounds.width;
      const dy = (e.clientY - d.y) * this.size.height / bounds.height;
      this.rect = d.corner ? resize(d.rect, d.corner, dx, dy, this.size, this.aspect, this.minimum.width, this.minimum.height) : move(d.rect, dx, dy, this.size);
      this.update();
    });
    const release = (e: PointerEvent) => {
      if (this.drag?.id !== e.pointerId) return;
      this.drag = undefined;
      if (this.box.hasPointerCapture(e.pointerId)) this.box.releasePointerCapture(e.pointerId);
      this.update(true);
    };
    this.box.addEventListener('pointerup', release); this.box.addEventListener('pointercancel', release);
    this.box.addEventListener('keydown', e => {
      const step = e.shiftKey ? 10 : 1;
      const delta: Record<string, [number, number]> = { ArrowLeft: [-step, 0], ArrowRight: [step, 0], ArrowUp: [0, -step], ArrowDown: [0, step] };
      const d = delta[e.key]; if (!d) return;
      e.preventDefault();
      const corner = (e.target as HTMLElement).dataset.corner as Corner | undefined;
      this.rect = corner ? resize(this.rect, corner, ...d, this.size, this.aspect, this.minimum.width, this.minimum.height) : move(this.rect, ...d, this.size);
      this.update(true);
    });
  }

  get active() { return this.dialog.open; }
  open(source: Source): Promise<Crop | undefined> {
    if (this.opening) return this.opening;
    this.source = source; this.size = { width: source.width, height: source.height };
    this.history=source.frames||[];this.selected=this.history.length-1;for(let i=this.history.length-1;i>=0;i--){if(this.history[i].report||this.history[i].regions?.length){this.selected=i;break;}}
    const chosen=this.history[this.selected];if(chosen){this.source={...source,image:chosen.image,width:chosen.width,height:chosen.height};this.size={width:chosen.width,height:chosen.height};}
    this.aspect = source.aspect === undefined ? 16 / 9 : source.aspect;
    this.minimum = source.minimum || { width: 320, height: 180 };
    this.original = { ...source.initial }; this.rect = selection(source.initial, this.size, this.aspect, this.minimum.width, this.minimum.height);
    this.frameCount = 0; this.lastPaint = -Infinity;
    document.getElementById('crop-description')!.textContent = source.description;
     document.getElementById('crop-title')!.textContent = source.frames?.length && source.purpose==='region' ? 'Inspect captured HUD elements' : source.purpose === 'region' ? 'Select field region' : 'Select your game viewport';
    document.getElementById('crop-move')!.setAttribute('aria-label', source.purpose === 'region' ? 'Move field region' : 'Move game viewport');
    document.querySelector('.crop-result p')!.textContent = source.purpose === 'region'
      ? 'Keep only the pixels for this field or search area inside the frame. Coordinates are relative to the analyzed image.'
      : 'Keep the entire game HUD inside the frame. Exclude chat, browser chrome and black bars.';
    this.preview.setAttribute('aria-label', 'Preview of the selected image region');
    document.getElementById('crop-source-kind')!.textContent = chosen ? 'Frozen source · sampling paused' : source.image instanceof HTMLVideoElement ? 'Live source preview' : 'Screenshot preview';
    document.getElementById('crop-history-controls')!.hidden=!this.history.length;
    document.getElementById('crop-history')!.replaceChildren();
    for(const [i,frame]of this.history.entries()){
      const b=document.createElement('button');b.type='button';b.className='crop-history-frame';b.setAttribute('aria-label',`Use captured frame ${i+1} of ${this.history.length}`);
      b.setAttribute('aria-pressed',String(i===this.selected));const thumb=document.createElement('canvas');thumb.width=128;thumb.height=Math.max(1,Math.round(128*frame.height/frame.width));thumb.getContext('2d')!.drawImage(frame.image,0,0,thumb.width,thumb.height);
      const caption=document.createElement('span');caption.textContent=`${i+1} · ${new Date(frame.timestamp).toLocaleTimeString()}`;b.append(thumb,caption);b.onclick=()=>this.selectFrame(i);document.getElementById('crop-history')!.append(b);
    }
    document.getElementById('crop-history-status')!.textContent=`Sampling paused · ${this.history.length} of the last 5 source frames available. Located boxes come from that frame; configured boxes are not detections.`;
    const ratio = this.aspect === null ? 'Free aspect' : Math.abs(this.aspect - 16 / 9) < .01 ? '16:9' : `${this.aspect.toFixed(2)}:1`;
    document.querySelector('#crop-move span')!.textContent = source.purpose === 'region' ? 'FIELD REGION' : `GAME VIEW · ${ratio.toUpperCase()}`;
    document.getElementById('crop-fit')!.textContent = this.aspect === null ? 'Use full source' : `Fit ${ratio}`;
    document.getElementById('crop-auto')!.hidden = source.purpose === 'region';
    document.getElementById('crop-apply')!.textContent = source.applyLabel;
    this.opening = new Promise(resolve => { this.resolve = resolve; });
    this.dialog.showModal(); this.update(true); this.paint(performance.now());
    document.getElementById('crop-move')!.focus();
    return this.opening;
  }
  private selectFrame(index:number){
    const f=this.history[index];if(!f||!this.source)return;const crop=normalized(this.rect,this.size);this.selected=index;
    this.source={...this.source,image:f.image,width:f.width,height:f.height};this.size={width:f.width,height:f.height};
    this.rect=selection(crop,this.size,this.aspect,this.minimum.width,this.minimum.height);this.lastPaint=-Infinity;
    document.querySelectorAll('#crop-history button').forEach((b,i)=>b.setAttribute('aria-pressed',String(i===index)));
    this.update(true);this.renderSource();
  }
  private overlays(){const f=this.history[this.selected];return f?reportRegions(f):this.source?.regions||[];}
  private renderSource(){
    if(!this.source)return;const width=Math.min(1440,this.size.width),height=Math.max(1,Math.round(width*this.size.height/this.size.width));
    this.canvas.width=width;this.canvas.height=height;const ctx=this.canvas.getContext('2d')!;ctx.drawImage(this.source.image,0,0,width,height);
    if((document.getElementById('crop-show-regions') as HTMLInputElement).checked)drawRegions(ctx,this.overlays(),width,height);
    const boxes=this.overlays();document.getElementById('crop-region-status')!.textContent=boxes.length
      ?`${boxes.filter(r=>r.origin==='detected').length} located boxes · ${boxes.filter(r=>r.origin==='configured').length} configured search regions`
      :'No detector report for this frame yet. Boxes are never invented from screen size.';
    const frame=this.history[this.selected];this.canvas.dataset.frame=String(frame?.id||1);this.canvas.dataset.sourceFrameId=String(frame?.id||'reference');
  }

  cancel() { if (this.resolve) this.finish(); }
  private finish(crop?: Crop) {
    const resolve = this.resolve; this.resolve = undefined; this.opening = undefined;
    cancelAnimationFrame(this.animation); this.source = undefined; this.drag = undefined;
    this.history=[];this.selected=-1;document.getElementById('crop-history')!.replaceChildren();
    this.dialog.close(); resolve?.(crop);
  }
  private announce(message: string) { document.getElementById('crop-editor-status')!.textContent = message; }

  private update(announce = false) {
    const { x, y, w, h } = normalized(this.rect, this.size);
    Object.assign(this.box.style, { left: `${x * 100}%`, top: `${y * 100}%`, width: `${w * 100}%`, height: `${h * 100}%` });
    this.stage.style.aspectRatio = `${this.size.width} / ${this.size.height}`;
    const summary = `${Math.round(this.rect.w)}×${Math.round(this.rect.h)} selected · left ${Math.round(this.rect.x)}, top ${Math.round(this.rect.y)} px · ${this.aspect === null ? 'free aspect' : 'aspect locked'}`;
    const previewScale = Math.min(640 / this.rect.w, 480 / this.rect.h);
    this.preview.width = Math.max(1, Math.round(this.rect.w * previewScale));
    this.preview.height = Math.max(1, Math.round(this.rect.h * previewScale));
    this.preview.style.maxHeight = '300px'; this.preview.style.objectFit = 'contain';
    this.preview.style.aspectRatio = `${this.rect.w} / ${this.rect.h}`;
    document.getElementById('crop-summary')!.textContent = summary;
    const historical=this.history[this.selected];document.getElementById('crop-source-size')!.textContent=historical&&(historical.rawWidth!==this.size.width||historical.rawHeight!==this.size.height)
      ?`${historical.rawWidth}×${historical.rawHeight} source · ${this.size.width}×${this.size.height} preview`:`${this.size.width}×${this.size.height} source`;
    let error = '';
    try { viewport(normalized(this.rect, this.size), this.size, this.aspect, this.minimum.width, this.minimum.height); } catch (e) { error = e instanceof Error ? e.message : String(e); }
    (document.getElementById('crop-apply') as HTMLButtonElement).disabled = !!error;
    if (announce) this.announce(error || summary);
    this.renderSource();
    this.drawPreview();
  }

  private drawPreview() {
    if (!this.source) return;
    const { x, y, w, h } = this.rect;
    try { this.preview.getContext('2d')!.drawImage(this.source.image, x, y, w, h, 0, 0, this.preview.width, this.preview.height); }
    catch { /* A disconnected/resizing stream may temporarily have no frame. */ }
    if((document.getElementById('crop-show-regions') as HTMLInputElement).checked)drawRegions(this.preview.getContext('2d')!,this.overlays(),this.preview.width,this.preview.height,normalized(this.rect,this.size));
  }
  private paint = (now: number) => {
    const source = this.source; if (!source || !this.dialog.open) return;
    if(!(source.image instanceof HTMLVideoElement)){this.renderSource();this.drawPreview();return;}
    if (now - this.lastPaint >= 125) {
      this.lastPaint = now;
      if (source.image instanceof HTMLVideoElement) {
        const { videoWidth: width, videoHeight: height } = source.image;
        if (width && height && (width !== this.size.width || height !== this.size.height)) {
          const crop = normalized(this.rect, this.size); this.size = { width, height };
          this.rect = selection(crop, this.size, this.aspect, this.minimum.width, this.minimum.height); this.update(); this.announce('Source size changed. Review the selection before applying.');
        }
      }
      const width = Math.min(1440, this.size.width), height = Math.max(1,Math.round(width * this.size.height / this.size.width));
      if (this.canvas.width !== width || this.canvas.height !== height) { this.canvas.width = width; this.canvas.height = height; }
      try {
        this.canvas.getContext('2d')!.drawImage(source.image, 0, 0, width, height);
        this.canvas.dataset.frame = String(++this.frameCount); this.drawPreview();
      } catch { /* Keep the last visible preview until the next video frame. */ }
    }
    this.animation = requestAnimationFrame(this.paint);
  };
}
