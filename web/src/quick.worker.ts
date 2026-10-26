/// <reference lib="webworker" />
import init,{QuickCoach} from '../wasm/coach_web';
import {loadVerifiedAsset} from './assets';
const ctx=self as unknown as DedicatedWorkerGlobalScope;
let coach:QuickCoach|undefined;
let mailbox=Promise.resolve();
ctx.onmessage=event=>{const m=event.data;mailbox=mailbox.then(async()=>{
  try{
    if(m.type==='init'){
      await init();const response=await fetch(`${m.base}assets/manifest.json`,{cache:'no-cache'});if(!response.ok)throw new Error(`Quick manifest: HTTP ${response.status}`);
      const manifest=await response.json();let cache:Cache|undefined;try{cache=await caches.open(`deadlock-assets-${manifest.version}`);}catch{/* optional storage */}
      const rec=await loadVerifiedAsset(m.base,'models/text-rec.rten',manifest.files['models/text-rec.rten'],cache);
      const assets=await loadVerifiedAsset(m.base,'templates.bin',manifest.files['templates.bin'],cache);
      coach=new QuickCoach(new Uint8Array(rec),new Uint8Array(assets));coach.configure(JSON.stringify(m.options));ctx.postMessage({type:'ready'});return;
    }
    if(!coach)return;
    if(m.type==='configure'){coach.configure(JSON.stringify(m.options));return;}
    if(m.type==='reset'){coach.reset();return;}
    if(m.type==='frame'){
      const start=performance.now();const analysis=coach.observe(new Uint8Array(m.rgba),m.width,m.height,JSON.stringify(m.context),m.timestamp);
      ctx.postMessage({type:'result',generation:m.generation,sourceId:m.sourceId,timestamp:m.timestamp,analysis,elapsed:performance.now()-start});
    }
  }catch(e){ctx.postMessage({type:'error',message:String(e)});}
});};
