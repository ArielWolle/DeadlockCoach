import type {Cue} from './types';
/** Convert a frame-time countdown to delivery time; never announce a three-second rotation. */
export function freshCue(cue:Cue,clock:number|null|undefined,ageMs:number):Cue|undefined{
  if(cue.key.startsWith('timer:')){
    if(clock==null||ageMs>30000)return;
    const spawn=Number(cue.key.slice(cue.key.lastIndexOf(':')+1));const remaining=Math.ceil(spawn-clock-ageMs/1000);
    if(!Number.isFinite(remaining)||remaining<15)return;
    return {...cue,text:cue.text.replace(/in \d+ seconds/,`in about ${remaining} seconds`),timer_spawn_ms:Date.now()+remaining*1000};
  }
  if(ageMs>(cue.key==='lane:back_off'?2500:15000))return;
  return cue;
}
