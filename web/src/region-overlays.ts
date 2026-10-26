import type { SourceFrame } from './frame-history';
import type {PerceptionReport} from './types';
export interface OverlayRegion {id:string;label:string;origin:'configured'|'detected';x:number;y:number;w:number;h:number;score?:number|null}
/** Every box is normalized to the captured source frame; the editor knows no game names. */
export function reportRegions(frame:SourceFrame):OverlayRegion[]{
  return frame.report?regionsFromReport(frame.report,frame.crop):frame.regions||[];
}
export function regionsFromReport(report:PerceptionReport,crop={x:0,y:0,w:1,h:1}):OverlayRegion[]{
  const [w,h]=report.frame_size;if(!w||!h)return[];
  const regions:OverlayRegion[]=[];const seen=new Set<string>();
  for(const stage of report.stages){
    const observations=stage.regions?.length?stage.regions:stage.bounds&&['crop','locate','locate_text'].includes(stage.operation)
      ?[{id:stage.id,label:stage.id,bounds:stage.bounds,origin:stage.bounds_origin||(stage.operation==='locate'?'detected':'configured'),score:stage.score}]:[];
    for(const r of observations){const b=r.bounds;const id=`${stage.id}/${r.id}`;if(seen.has(id))continue;seen.add(id);
      regions.push({id,label:r.label,origin:r.origin,x:crop.x+b.x/w*crop.w,y:crop.y+b.y/h*crop.h,
        w:b.w/w*crop.w,h:b.h/h*crop.h,score:r.score});}
  }return regions;
}
export function drawRegions(ctx:CanvasRenderingContext2D,regions:OverlayRegion[],width:number,height:number,crop?:{x:number;y:number;w:number;h:number}){
  ctx.save();const font=Math.max(11,12*width/(ctx.canvas.clientWidth||width));ctx.font=`${font}px system-ui`;ctx.lineWidth=Math.max(2,width/(ctx.canvas.clientWidth||width));
  for(const r of regions){let{x,y,w,h}=r;if(crop){x=(x-crop.x)/crop.w;y=(y-crop.y)/crop.h;w/=crop.w;h/=crop.h;}
    const px=x*width,py=y*height,pw=w*width,ph=h*height;
    if(px+pw<0||py+ph<0||px>width||py>height)continue;
    ctx.strokeStyle=r.origin==='detected'?'#75f2f1':'#f0c778';ctx.setLineDash(r.origin==='configured'?[5,4]:[]);ctx.strokeRect(px,py,pw,ph);
    const text=`${r.origin==='detected'?'Located':'Search'} · ${r.label}`;const tw=Math.min(ctx.measureText(text).width+10,width);
    const tx=Math.max(0,Math.min(width-tw,px)),ty=Math.max(font+3,Math.min(height,py));ctx.fillStyle='#081318e6';ctx.fillRect(tx,ty-font-3,tw,font+5);
    ctx.fillStyle=ctx.strokeStyle;ctx.fillText(text,tx+4,ty-2,tw-8);
  }ctx.restore();
}
