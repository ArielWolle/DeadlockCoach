import type { Crop } from './crop';
import type { PerceptionReport } from './types';
import type {OverlayRegion} from './region-overlays';
export interface SourceFrame {
  id:number;timestamp:number;image:ImageBitmap;width:number;height:number;rawWidth:number;rawHeight:number;crop:Crop;report?:PerceptionReport|null;regions?:OverlayRegion[];
}
/** Source-provider-neutral, bounded in-memory history. No automatic disk/network recording. */
export class FrameHistory {
  private frames:SourceFrame[]=[];private nextId=0;
  add(frame:Omit<SourceFrame,'id'>){const item={...frame,id:++this.nextId};this.frames.push(item);while(this.frames.length>5)this.frames.shift()!.image.close();return item.id;}
  attach(id:number,report?:PerceptionReport|null){const frame=this.frames.find(f=>f.id===id);if(frame)frame.report=report;}
  attachRegions(id:number,regions:OverlayRegion[]){const frame=this.frames.find(f=>f.id===id);if(frame)frame.regions=regions;}
  snapshot(){return this.frames.map(f=>({...f,crop:{...f.crop}}));}
  clear(){for(const f of this.frames)f.image.close();this.frames=[];}
}
