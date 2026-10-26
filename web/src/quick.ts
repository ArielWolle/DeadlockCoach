import type {Analysis,PerceptionReport,Snapshot} from './types';
import type {Pixels} from './capture';
type Bounds={x:number;y:number;w:number;h:number};
interface Context{snapshot:Snapshot;timestamp_ms:number;frame_size:[number,number];game:Bounds;hp:Bounds;map:Bounds}
/** One in-flight fast frame; busy frames are skipped, never queued behind stale pixels. */
export class QuickMonitor{
  private worker?:Worker;private ready=false;private busy=false;private context?:Context;private pendingContext?:Context;private deadline?:ReturnType<typeof setTimeout>;
  lastResultAt=0;elapsed=0;
  onResult: (analysis:Analysis,generation:number,timestamp:number,elapsed:number,sourceId:number|undefined,context:Context)=>void=()=>{};
  onError:(message:string)=>void=()=>{};
  start(base:string,options:unknown){
    if(this.worker)return;
    this.worker=new Worker(new URL('./quick.worker.ts',import.meta.url),{type:'module'});
    const fail=(message:string)=>{this.stop();this.onError(message);};
    this.worker.onerror=e=>{e.preventDefault();fail(e.message||'Quick monitor failed');};
    this.deadline=setTimeout(()=>fail('Quick monitor initialization timed out'),120000);
    this.worker.onmessage=e=>{
      const m=e.data;
      if(m.type==='ready'){clearTimeout(this.deadline);this.ready=true;return;}
      if(m.type==='error'){fail(m.message);return;}
      if(m.type==='result'){clearTimeout(this.deadline);this.busy=false;this.elapsed=m.elapsed;
        if(m.analysis&&this.pendingContext){this.lastResultAt=Date.now();this.onResult(m.analysis,m.generation,m.timestamp,m.elapsed,m.sourceId,this.pendingContext);}this.pendingContext=undefined;}
    };
    this.worker.postMessage({type:'init',base,options});
  }
  configure(options:unknown){this.worker?.postMessage({type:'configure',options});}
  seed(snapshot:Snapshot,report:PerceptionReport){
    const game=report.stages.flatMap(s=>s.regions||[]).find(r=>r.id==='game_context')?.bounds;
    const hp=report.fields['player.hp']?.bounds;const map=report.fields['world.minimap']?.bounds;
    this.context=game&&hp&&map&&snapshot.screen==='Gameplay'&&!snapshot.dead
      ?{snapshot:structuredClone(snapshot),timestamp_ms:report.timestamp_ms,frame_size:report.frame_size,game,hp,map}:undefined;
  }
  sample(p:Pixels,generation:number){
    if(!this.ready||this.busy||!this.context||p.timestamp-this.context.timestamp_ms>30000)return;
    this.busy=true;this.pendingContext=this.context;const rgba=new Uint8ClampedArray(p.rgba.data);
    this.deadline=setTimeout(()=>{this.stop();this.onError('Quick monitor exceeded its five-second budget');},5000);
    this.worker!.postMessage({type:'frame',rgba:rgba.buffer,width:p.rgba.width,height:p.rgba.height,context:this.context,timestamp:p.timestamp,sourceId:p.sourceId,generation},[rgba.buffer]);
  }
  reset(){this.context=undefined;this.lastResultAt=0;this.worker?.postMessage({type:'reset'});}
  stop(){clearTimeout(this.deadline);this.worker?.terminate();this.worker=undefined;this.ready=false;this.busy=false;this.context=undefined;this.lastResultAt=0;}
}
