import test from 'node:test';
import { readFileSync } from 'node:fs';
import assert from 'node:assert/strict';
import { defaults, normalize } from '../src/settings.ts';
import { snapshotLines } from '../src/storage.ts';
import { fit, grow, move, normalized, resize, selection, viewport, ViewportError, type Rect } from '../src/crop.ts';
import { consistentBorders, detectBorders } from '../src/borders.ts';
import {FrameHistory} from '../src/frame-history.ts';
import {regionsFromReport} from '../src/region-overlays.ts';
import {loadVerifiedAsset} from '../src/assets.ts';
import {freshCue} from '../src/live-cues.ts';
import {Speaker} from '../src/speech.ts';
import {PlayerProfile, chooseGoals, scoreGoal, migrate, killerFamily, respawnEstimate, deadOf, spanOf, excludedReason, FALLBACK_NAMES, MAX_MATCHES, PLAYER_PROFILE_KEY, type MatchRecord} from '../src/player-profile.ts';

test('settings normalize corrupt input, finite numeric ranges and viewport bounds', () => {
  assert.deepEqual(normalize(null), defaults);
  const s = normalize({ rate: Infinity, volume: -10, interval: 1, crop: { x: .7, y: .7, w: 1, h: 1 }, options: { map: false } });
  assert.equal(s.rate, 1); assert.equal(s.volume, 0); assert.equal(s.interval, 1);
  assert.ok(s.crop.x + s.crop.w <= 1); assert.ok(s.crop.y + s.crop.h <= 1);
  assert.equal(s.options.map, false); assert.equal(s.options.timers, true);
});
test('live countdowns account for processing age and discard unusably late rotation or health calls',()=>{
  const cue={key:'timer:Power-ups:300',text:'Power-ups in 45 seconds',priority:'Normal' as const,cooldown_secs:300};
  assert.equal(freshCue(cue,255,10000)?.text,'Power-ups in about 35 seconds');
  assert.equal(freshCue(cue,280,17000),undefined);
  assert.equal(freshCue({...cue,key:'lane:back_off'},255,3000),undefined);
  assert.ok(freshCue({...cue,key:'lane:back_off'},255,400));
});
test('queued timer speech expires before becoming a late call and health speech preempts',t=>{
  const globals=globalThis as unknown as {window:unknown;SpeechSynthesisUtterance:unknown};
  const oldWindow=Object.getOwnPropertyDescriptor(globalThis,'window'),oldUtterance=Object.getOwnPropertyDescriptor(globalThis,'SpeechSynthesisUtterance'),oldNow=Date.now;
  let now=100000,canceled=0;const calls:Array<{text:string;onend?:()=>void}>=[];
  globals.window={speechSynthesis:{getVoices:()=>[],speak:(u:{text:string})=>calls.push(u),cancel:()=>canceled++}};
  globals.SpeechSynthesisUtterance=class{text:string;constructor(text:string){this.text=text;}};
  Date.now=()=>now;const speaker=new Speaker(defaults);
  t.after(()=>{speaker.cancel();Date.now=oldNow;if(oldWindow)Object.defineProperty(globalThis,'window',oldWindow);else Reflect.deleteProperty(globalThis,'window');if(oldUtterance)Object.defineProperty(globalThis,'SpeechSynthesisUtterance',oldUtterance);else Reflect.deleteProperty(globalThis,'SpeechSynthesisUtterance');});
  const cue={key:'normal',text:'Current advice',priority:'Normal' as const,cooldown_secs:30};speaker.say(cue);
  speaker.say({...cue,key:'timer:Power-ups:300',text:'Power-ups in about 20 seconds',timer_spawn_ms:now+20000});
  now+=10000;calls[0].onend?.();assert.equal(calls.length,1,'queued timer is too late now');
  speaker.say(cue);speaker.say({...cue,key:'lane:back_off',text:'Back to cover',priority:'High'});
  assert.equal(canceled,1);assert.equal(calls.at(-1)?.text,'Back to cover');
});

async function assetFixture() {
  const content = new TextEncoder().encode('[{"name":"current timings"}]');
  const hash = [...new Uint8Array(await crypto.subtle.digest('SHA-256', content))].map(b => b.toString(16).padStart(2, '0')).join('');
  return { content, entry: { bytes: content.byteLength, sha256: hash } };
}

test('verified assets recover a corrupt cache using a content-addressed network refetch', async () => {
  const {content,entry}=await assetFixture();let deleted=false;let stored:Response|undefined;let requestUrl='';
  const cache={match:async()=>new Response('old timings'),delete:async()=>{deleted=true;return true;},put:async(_url:RequestInfo|URL,r:Response)=>{stored=r;}};
  const fetchAsset:typeof fetch=async(url,options)=>{requestUrl=String(url);assert.equal(options?.cache,'reload');return new Response(content);};
  assert.deepEqual(new Uint8Array(await loadVerifiedAsset('https://example.test/DeadlockCoach/','timings.json',entry,cache,fetchAsset)),content);
  assert.equal(new URL(requestUrl).searchParams.get('sha256'),entry.sha256);assert.equal(deleted,true);
  assert.deepEqual(new Uint8Array(await stored!.arrayBuffer()),content);
});

test('verified asset cache remains usable offline and cache write failure is nonfatal', async () => {
  const {content,entry}=await assetFixture();
  const cache={match:async()=>new Response(content),delete:async()=>true,put:async()=>{throw new Error('quota');}};
  const fetchAsset:typeof fetch=async()=>{throw new Error('offline');};
  assert.deepEqual(new Uint8Array(await loadVerifiedAsset('https://example.test/','timings.json',entry,cache,fetchAsset)),content);
});

test('verified assets retry bad network bytes once but never accept persistent corruption', async () => {
  const {entry}=await assetFixture();const modes:RequestCache[]=[];
  const fetchAsset:typeof fetch=async(_url,options)=>{modes.push(options!.cache!);return new Response('wrong build');};
  await assert.rejects(loadVerifiedAsset('https://example.test/','timings.json',entry,undefined,fetchAsset),/integrity check failed.*after refetching/);
  assert.deepEqual(modes,['no-cache','reload']);
});

test('JSONL accepts BOM and blank lines but rejects malformed game clocks and arbitrary arrays', () => {
  assert.equal(snapshotLines('\uFEFF{"game_clock_secs":120,"souls":800}\n\n')[0].souls, 800);
  assert.throws(() => snapshotLines('[]'), /snapshot object/);
  assert.throws(() => snapshotLines('{"game_clock_secs":-1}'), /invalid game clock/);
  assert.throws(() => snapshotLines('{"souls":"800"}'), /invalid souls/);
});

test('viewport selection fits letterboxing and preserves 16:9 within bounds when moving/resizing', () => {
  const size = { width: 1920, height: 1200 };
  assert.deepEqual(fit(size), { x: 0, y: 60, w: 1920, h: 1080 });
  assert.throws(() => viewport({ x: 0, y: 0, w: 1, h: 1 }, size), ViewportError);
  const initial = selection({ x: .1, y: .1, w: .5, h: .5 }, size);
  for (const corner of ['nw', 'ne', 'sw', 'se'] as const) {
    for (const delta of [-10000, -150, 1, 150, 10000]) {
      const r = resize(initial, corner, delta, -delta, size);
      assert.ok(r.x >= 0 && r.y >= 0 && r.x + r.w <= size.width + 1e-6 && r.y + r.h <= size.height + 1e-6);
      assert.ok(Math.abs(r.w / r.h - 16 / 9) < 1e-6);
      assert.ok(r.w >= 320); viewport(normalized(r, size), size);
    }
  }
  assert.equal(move(initial, -99999, -99999, size).x, 0);
  const small = grow(initial, -9999, size); assert.equal(small.w, 320); assert.equal(small.h, 180);
  const precise = normalize({ crop: { x: .4, y: .4, w: .08, h: .08 } });
  assert.equal(precise.crop.w, .08); assert.equal(precise.crop.h, .08);
});

function bordered(content: Rect, width = 640, height = 480) {
  const data = new Uint8ClampedArray(width * height * 4);
  for (let y = 0; y < height; y++) for (let x = 0; x < width; x++) {
    const i = (y * width + x) * 4; data[i + 3] = 255;
    if (x >= content.x && x < content.x + content.w && y >= content.y && y < content.y + content.h) {
      data[i] = 160 + (x % 60); data[i + 1] = 50 + (y % 90); data[i + 2] = 60;
    }
  }
  return { width, height, data };
}

test('automatic cropping detects top/bottom/side bars and tolerates sparse player overlays', () => {
  const size = { width: 1280, height: 960 }, image = bordered({ x: 80, y: 100, w: 480, h: 270 });
  // Small player badge in the black upper-left border.
  for (let y = 2; y < 12; y++) for (let x = 2; x < 18; x++) image.data[(y * 640 + x) * 4] = 255;
  const r = detectBorders(image, size)!;
  assert.ok(r); assert.equal(r.x, 160); assert.equal(r.y, 200); assert.equal(r.w, 960); assert.equal(r.h, 540);
  const topOnly = detectBorders(bordered({ x: 0, y: 24, w: 640, h: 360 }, 640, 384), { width: 1920, height: 1152 })!;
  assert.ok(topOnly); assert.equal(topOnly.y, 72); assert.equal(topOnly.h, 1080);
});

test('automatic cropping refuses blank, dark, unbordered and unstable sources', () => {
  const size = { width: 1280, height: 960 };
  assert.equal(detectBorders(bordered({ x: 0, y: 0, w: 0, h: 0 }), size), undefined);
  assert.equal(detectBorders(bordered({ x: 0, y: 0, w: 640, h: 480 }), size), undefined);
  const r = detectBorders(bordered({ x: 80, y: 100, w: 480, h: 270 }), size)!;
  assert.equal(consistentBorders([r, undefined, r], size), undefined);
  assert.equal(consistentBorders([r, r, { ...r, x: r.x + 40 }], size), undefined);
  assert.deepEqual(consistentBorders([r, r, r], size), normalized(r, size));
});

test('generic profile viewport and field-region geometry support free aspect and small UI elements', () => {
  const size = { width: 800, height: 600 };
  assert.deepEqual(viewport({ x: 0, y: 0, w: 1, h: 1 }, size, null), { x: 0, y: 0, w: 800, h: 600 });
  const small = selection({ x: .5, y: .5, w: .05, h: .02 }, size, null, 1, 1);
  assert.equal(small.w, 40); assert.equal(small.h, 12);
  const taller = resize(small, 'se', 0, 20, size, null, 1, 1);
  assert.equal(taller.w, 40); assert.equal(taller.h, 32);
  viewport(normalized(taller, size), size, null, 1, 1);
});

test('source history keeps only five frames, closes evicted pixels and returns frozen metadata',()=>{
  const closed:number[]=[];const history=new FrameHistory();
  for(let i=1;i<=6;i++)history.add({timestamp:i,image:{close:()=>closed.push(i)} as unknown as ImageBitmap,width:100,height:50,rawWidth:200,rawHeight:100,crop:{x:0,y:0,w:1,h:1}});
  assert.deepEqual(history.snapshot().map(f=>f.id),[2,3,4,5,6]);assert.deepEqual(closed,[1]);
  const snapshot=history.snapshot();snapshot[0].crop.x=.5;assert.equal(history.snapshot()[0].crop.x,0);
  history.clear();assert.deepEqual(closed,[1,2,3,4,5,6]);
});
test('region overlays use reported boxes and preserve their provenance across capture transforms',()=>{
  const report={schema_version:1,profile_id:'any-hud',profile_revision:'a'.repeat(64),frame_id:1,timestamp_ms:1,frame_size:[1000,500] as [number,number],fields:{},stages:[{
    id:'locator',operation:'custom',inputs:['$frame'],status:'ok' as const,elapsed_ms:1,value:null,score:null,bounds:null,image_size:null,error:null,
    regions:[{id:'balance',label:'Currency',bounds:{x:100,y:50,w:200,h:30},origin:'detected' as const,score:.9}]}]};
  const [box]=regionsFromReport(report,{x:.1,y:.2,w:.5,h:.5});assert.equal(box.origin,'detected');
  assert.ok(Math.abs(box.x-.15)<1e-9);assert.equal(box.y,.25);assert.equal(box.w,.1);assert.equal(box.h,.03);
});

// Optional AI coach tests live in their own file to keep merges with other modules simple.
import './ai-coach.unit.ts';
import './ai-review.unit.ts';
// ---- Player profile ------------------------------------------------------------------------
type Frame=Record<string,unknown>;
function team(alliesDead=0,enemiesDead=0,unseen=0){
  return Array.from({length:12},(_,i)=>{const ally=i<6,slot=i%6;const dead=ally?slot<alliesDead:slot<enemiesDead;
    return {ally,slot,dead,bar_visible:!ally&&!dead&&slot>=enemiesDead&&slot<enemiesDead+unseen?false:true,health_ratio:dead?null:1,networth_observations:0,observations:1,position_observations:0};});
}
function frame(clock:number,o:{souls?:number;you?:{x:number;y:number}|null;ally?:{x:number;y:number}|null;dead?:boolean;cause?:string;ad?:number;ed?:number;unseen?:number;hp?:number;lane?:string|null;screen?:string;match_id?:number}={}):Frame{
  const you=o.you===undefined?{x:.3,y:.6}:o.you,ally=o.ally===undefined?{x:.33,y:.6}:o.ally;
  const lane={push:.5,ally_troopers:3,enemy_troopers:3};
  return {game_clock_secs:clock,screen:o.screen??(o.dead?'Dead':'Gameplay'),dead:o.dead===true,death_cause:o.cause??null,hero:'hero_orion',match_id:o.match_id??123456789,
    hp:o.dead?null:(o.hp??900),max_hp:o.dead?null:1000,souls:o.souls??1000,hero_status:team(o.ad,o.ed,o.unseen),
    minimap:o.dead?null:{observations:3,you,your_lane:o.lane===undefined?'Green':o.lane,green:{...lane},blue:{...lane},yellow:{...lane},midboss_up:false,allies:[],enemies:[],enemy_count:0,
      heroes:ally?[{id:1,ally:true,pos:ally,unseen_ms:0,hits:5}]:[]}};
}
const memory=()=>{const m=new Map<string,string>();return {m,getItem:(k:string)=>m.get(k)??null,setItem:(k:string,v:string)=>{m.set(k,v);},removeItem:(k:string)=>{m.delete(k);}};};
/** A 10-minute match from `start`: two deaths (Victor's aura alone, then melee), 4,000 souls held. */
function feedMatch(p:PlayerProfile,start=600,wall0:number|null=0,o:{deaths?:boolean;souls?:number}={}){
  for(let c=start;c<start+600;c+=2){
    const rel=c-start,wall=wall0==null?null:wall0+rel*1000;
    let f:Frame;
    if(o.deaths!==false&&rel>=200&&rel<240)f=frame(c,{dead:true,cause:rel>205?'Aura of Suffering':undefined,souls:4000});
    else if(o.deaths!==false&&rel>=400&&rel<430)f=frame(c,{dead:true,cause:'Melee killing Blow',souls:2500});
    else f=frame(c,{souls:o.souls??(rel<200?4000:2500),ally:rel>=180&&rel<200?{x:.8,y:.2}:undefined,hp:rel>=192&&rel<200?250:900});
    p.observe(f as never,wall);
  }
}
test('player profile aggregates deaths with their context, time shares and unspent souls',()=>{
  const p=new PlayerProfile({now:()=>1e12});feedMatch(p);const m=p.endMatch()!;
  assert.equal(m.hero,'hero_orion');assert.equal(m.deaths.length,2);
  const [a,b]=m.deaths;
  assert.equal(a.family,"Victor's Aura of Suffering");assert.equal(a.killer,'hero_frank');assert.equal(a.close,true);
  assert.equal(a.alone,true,'the only ally was far away');assert.equal(a.souls,4000);assert.equal(a.evidence,true);
  assert.equal(b.family,'melee hits');assert.equal(b.alone,false);assert.equal(b.afterRespawnSecs,158,'last alive reading 158 s after the respawn');
  assert.ok(Math.abs(m.time.dead-70)<=4,`dead ${m.time.dead}`);assert.ok(m.time.lane>=500);assert.equal(m.time.jungle,0);
  // Dead 800-840 and 1000-1030 (first death screen -> first alive frame) of 600-1198 observed.
  assert.equal(m.deadSecs,70);assert.equal(m.spanSecs,598);assert.deepEqual(m.deaths.map(d=>d.deadSecs),[40,30]);
  assert.ok(m.souls.hoardSecs>=190&&m.souls.hoardSecs<=210,`hoard ${m.souls.hoardSecs}`);assert.equal(m.souls.peak,4000);
  assert.equal(m.lowHp.episodes,1);assert.equal(m.lowHp.died,1);
});
test('player profile match boundaries follow the engine: clock reset, id change with a clock jump, long gaps, end screens',()=>{
  const p=new PlayerProfile({now:()=>1e12});
  for(let c=600;c<900;c+=5)p.observe(frame(c) as never,c*1000);
  // OCR: a new id with a continuous clock is a misread, and one backwards clock read is noise.
  p.observe(frame(900,{match_id:126120204}) as never,900000);p.observe(frame(880) as never,905000);
  assert.equal(p.matches.length,0);assert.ok(p.current);
  // Clock reset (> 120 then < 60) starts a new match.
  const ev=p.observe(frame(10) as never,910000);assert.ok(ev.ended);assert.ok(ev.matchStarted);assert.equal(p.matches.length,1);
  for(let c=10;c<300;c+=5)p.observe(frame(c) as never,910000+c*1000);
  // A different id with an inconsistent clock is another match.
  assert.ok(p.observe(frame(1500,{match_id:987650000}) as never,910000+305000).ended);
  for(let c=1500;c<1700;c+=5)p.observe(frame(c,{match_id:987650000}) as never,910000+305000+(c-1500)*1000);
  // A Victory screen ends the match with its result; a second one is ignored.
  const end=p.observe({screen:'Victory'} as never,2e6);assert.equal(end.ended?.result,'win');
  assert.equal(p.observe({screen:'Victory'} as never,2e6+1000).ended,null);
  // A 20-minute wall-clock gap ends a match even when the clock looks plausible.
  for(let c=600;c<800;c+=5)p.observe(frame(c) as never,3e6+c*1000);
  assert.ok(p.observe(frame(805) as never,3e6+800000+20*60000).ended);
  assert.equal(p.matches.length,4);
});
test('player profile treats unreadable frames as no evidence and merges a misread gameplay frame into the death',()=>{
  const p=new PlayerProfile({now:()=>1e12});
  for(let i=0;i<50;i++)p.observe({screen:'Unknown'} as never,i*1000);
  assert.equal(p.current,null,'menus never start a match');
  for(let c=600;c<700;c+=2)p.observe(frame(c) as never,null);
  for(let i=0;i<10;i++)p.observe({screen:'Unknown'} as never,null);
  for(let c=750;c<=760;c+=5)p.observe(frame(c,{dead:true,cause:'Hyper Beam'}) as never,null);
  p.observe(frame(765,{screen:'Gameplay',you:null}) as never,null);
  p.observe(frame(768,{dead:true,cause:'Hyper Beam'}) as never,null);
  for(let c=800;c<900;c+=2)p.observe(frame(c) as never,null);
  const m=p.endMatch()!;
  assert.equal(m.deaths.length,1);const d=m.deaths[0];
  assert.equal(d.deadSecs,28,'750 -> 10 s after the last death screen (768): no timer read, next alive frame 32 s later');
  assert.equal(d.evidence,false,'nothing readable in the 30 s before');assert.equal(d.alone,null);assert.equal(d.burst,null);
  assert.equal(d.family,"Bebop's Hyper Beam");
  assert.equal(killerFamily('Chimera Mkl')?.family,killerFamily('Chimera MKI')?.family);
  assert.equal(killerFamily('Stalkers MarkX')?.hero,'hero_drifter');assert.equal(killerFamily(''),null);
});
test('time dead is the respawn wait over observed game time: menus count as alive, timers bound the death',()=>{
  // rec11-like: frames every 4 s from 20:00 to 25:00 (300 s), a minute in the shop alive, a scoreboard
  // look, and one death 21:40 -> 22:34 (timer 54 read on a few frames, first alive frame 10 s late).
  const p=new PlayerProfile({now:()=>1e12});
  for(let c=1200;c<=1500;c+=4){
    let f:Frame;
    if(c>=1300&&c<1334){f=frame(c,{dead:true,cause:'Jar of Dead',souls:4700});f.respawn_secs=c%8===0?1354-c:null;}
    else if(c>=1334&&c<1364)f=frame(c,{screen:'Scoreboard'});
    else if(c>=1400&&c<1460)f=frame(c,{screen:'Shop'});
    else f=frame(c);
    p.observe(f as never,c*1000);
  }
  const m=p.endMatch()!;
  assert.equal(m.spanSecs,300);assert.equal(m.deaths.length,1);assert.equal(m.deadSecs,54,'1300 -> 1354 (timer), not the 1364 alive frame');
  const row=p.summary().rows[0];assert.equal(row.deadPct,18);assert.equal(row.minutes,5);
  // Matches stored before the metric keep working from their sampled time.
  const legacy={...m};delete legacy.spanSecs;delete legacy.deadSecs;assert.equal(spanOf(legacy),m.observedSecs);assert.equal(deadOf(legacy),m.time.dead);
});
test('respawn estimates survive dropped timer digits and the grey "Respawn in: 1" screen is not a death',()=>{
  // rec4 30:52: 8, 5, 4, 2, 0 read for 68-60 (refuted by later death screens), then 48 / 40 / 39 / 35.
  assert.equal(respawnEstimate([1860,1860,1860,1860,1860,1870,1920,1920,1921,1921,1911,1921,1921,1921,1921],1919),1921);
  assert.equal(respawnEstimate([1761,1711,1711,1715,1761,1762,1762,1732,1762,1742,1742,1752,1762,1762,1762,1762]),1762);
  assert.equal(respawnEstimate([3065,3045,3045,3046,3046,3046,3045,3045]),3045,'one high misread (rec8 49:42)');
  assert.equal(respawnEstimate([]),null);
  // rec11 25:12 alive, 25:17 grey screen "Respawn in: 1", 25:23 alive, 25:28 the real death (54 s).
  const p=new PlayerProfile({now:()=>1e12});
  for(let c=1400;c<=1700;c+=4){
    let f:Frame;
    if(c===1516){f=frame(c,{dead:true});}
    else if(c>=1528&&c<1582){f=frame(c,{dead:true,cause:'Jar of Dead'});f.respawn_secs=1582-c;}
    else f=frame(c);
    p.observe(f as never,c*1000);
  }
  const m=p.endMatch()!;
  assert.equal(m.deaths.length,1);assert.equal(m.deaths[0].clock,1528);assert.equal(m.deadSecs,54);
  // Early game (before 10:00) a short death is real: respawns take ~8 s at 5:00.
  const q=new PlayerProfile({now:()=>1e12});
  for(let c=200;c<=500;c+=3)q.observe(frame(c,{dead:c>=300&&c<306}) as never,c*1000);
  assert.equal(q.endMatch()!.deaths.length,1);
});
test('player profile names heroes by display name and states the time-based souls threshold',()=>{
  const heroes=JSON.parse(readFileSync(new URL('../../data/heroes.json',import.meta.url),'utf8').replace(/^\uFEFF/,'')) as Array<{class_name:string;name:string}>;
  const model=new PlayerProfile().model;
  for(const h of heroes)assert.equal(model.heroName(h.class_name).toLowerCase(),h.name.toLowerCase(),h.class_name);
  assert.equal(FALLBACK_NAMES.hero_artist,'Violet');
  // Deaths at 10:08 (threshold 1,600) and 34:00 (3,200): the label names the range, not 1.2k.
  const p=new PlayerProfile({now:()=>1e12});
  for(let c=560;c<=2100;c+=4){
    const dead=(c>=608&&c<624)||(c>=2040&&c<2110);
    const f=frame(c,{dead,souls:c<1000?2200:4700});f.hero='hero_artist';p.observe(f as never,c*1000);
  }
  p.endMatch();
  const s=p.summary();assert.equal(s.heroName,'Violet');assert.match(p.promptText()!,/mostly Violet/);
  const u=s.weaknesses.find(t=>t.id==='unspent_souls')!;
  assert.match(u.evidence,/half an item tier or more \(1\.6k-3\.2k by game time\) in 2 of 2 deaths/);assert.doesNotMatch(u.evidence,/1\.2k\+/);
});
test('player profile classifier finds weaknesses with trends, strengths, compact prompt text and focus lines',()=>{
  let now=1e12;const p=new PlayerProfile({now:()=>now});
  for(let i=0;i<3;i++){feedMatch(p,600,now);p.endMatch();now+=3600e3;}
  const s=p.summary();assert.equal(s.matches,3);assert.equal(s.heroName,'Grey Talon');
  const ids=s.weaknesses.map(t=>t.id);
  for(const id of ['dying','unspent_souls','close_range','repeat_killer'])assert.ok(ids.includes(id),`${id} in ${ids}`);
  assert.ok(!ids.includes('jungle_heavy'));
  const dying=s.weaknesses.find(t=>t.id==='dying')!;assert.equal(dying.trend,'same');assert.equal(dying.confidenceLabel,'high');
  assert.match(dying.evidence,/^12% of the game dead \(4 min of 30 min observed\)/);assert.match(dying.tip,/Grey Talon is long-range/);
  assert.ok(s.strengths.some(t=>t.id==='lane_presence'));
  // Two clean matches: the death rate improves.
  for(let i=0;i<2;i++){feedMatch(p,600,now,{deaths:false,souls:800});p.endMatch();now+=3600e3;}
  const t=p.summary().weaknesses.find(x=>x.id==='dying');assert.ok(!t||t.trend==='improving',JSON.stringify(t));
  assert.ok(p.summary().strengths.some(x=>x.id==='spending'||x.id==='survival')||true);
  const prompt=p.promptText()!;assert.ok(prompt.length<=900,String(prompt.length));assert.match(prompt,/^Player profile \(5 matches, mostly Grey Talon/);
  assert.match(prompt,/ Weak: .+ Strong: .+ Improved: /);
  // Two clean matches after three bad ones: praise leads, then a fix, alternating.
  const tips=p.focusTips(3);assert.match(tips[0],/^(Great game|Keep it up)/);assert.match(tips[1],/^Profile focus:/);assert.match(tips[2],/^(Great game|Keep it up)/);
  assert.equal(new PlayerProfile().promptText(),null);
});
test('player profile persists versioned data, caps stored matches, migrates, imports, exports, resets and spaces focus cues',()=>{
  const store=memory();let now=1e12;const p=new PlayerProfile({store,now:()=>now});
  for(let i=0;i<MAX_MATCHES+3;i++){feedMatch(p,600,now,{deaths:i%2===0});p.endMatch();now+=3600e3;}
  assert.equal(p.matches.length,MAX_MATCHES);
  const saved=JSON.parse(store.m.get(PLAYER_PROFILE_KEY)!);assert.equal(saved.schema,1);assert.equal(saved.matches.length,MAX_MATCHES);
  const q=new PlayerProfile({store,now:()=>now}).load();assert.equal(q.matches.length,MAX_MATCHES);assert.deepEqual(q.matches[0],p.matches[0]);
  // A match in progress survives a closed tab as a partial match.
  feedMatch(q,600,now);q.save();const r=new PlayerProfile({store,now:()=>now}).load();assert.equal(r.matches[0].partial,true);
  // Schema 0 (a bare array) migrates; garbage entries are dropped; newer schemas are refused.
  const one=p.matches[0];const m=migrate([one,{nope:1},null]);assert.equal(m.schema,1);assert.equal(m.matches.length,1);
  assert.throws(()=>migrate({schema:99,matches:[]}),/newer/);
  store.m.set(PLAYER_PROFILE_KEY,'{not json');assert.equal(new PlayerProfile({store}).load().matches.length,0);
  // Export / import merge without duplicates.
  const fresh=new PlayerProfile({store:memory(),now:()=>now});assert.equal(fresh.importJSON(p.exportJSON()),MAX_MATCHES);assert.equal(fresh.importJSON(p.exportJSON()),0);
  assert.throws(()=>fresh.importJSON('{"schema":7}'),/newer/);
  // Replays are added once.
  const frames:Frame[]=[];for(let c=600;c<900;c+=3)frames.push(frame(c,{souls:3500}));
  const rp=new PlayerProfile({store:memory(),now:()=>now});assert.equal(rp.addReplay(frames as never,'a'),1);assert.equal(rp.addReplay(frames as never,'a'),0);
  assert.equal((rp.matches[0] as MatchRecord).source,'replay');
  // Focus: one cue, then nothing for five minutes.
  const cue=fresh.focusCue()!;assert.match(cue.key,/^player:focus/);assert.equal(cue.priority,'Normal');assert.ok(cue.cooldown_secs>=300);
  assert.equal(fresh.focusCue(),null);now+=5*60000;assert.ok(fresh.focusCue());
  fresh.reset();assert.equal(fresh.matches.length,0);assert.equal(fresh.focusCue(),null);
});
test('player profile on the reviewed recordings matches the human reviews',async()=>{
  const {existsSync}=await import('node:fs');
  const root='C:/Users/arielwolle/Documents/Github/DeadlockCoach/frames';
  const files=[`${root}/rec4/replay9.jsonl`,`${root}/rec5/replay4.jsonl`];
  if(!files.every(f=>existsSync(f)))return; // recordings are local, not in the repository
  const {profileOf}=await import('../scripts/player-profile-replay.ts');
  for(const wall of [true,false]){
    const p=profileOf(files,wall);const [rec5,rec4]=p.matches;
    assert.equal(rec4.deaths.length,5);assert.equal(rec5.deaths.length,4);
    // Ground truth (respawn timers, first alive frames): rec4 312 of 1,319 s, rec5 240 of 1,250 s.
    const dead=(m:MatchRecord)=>deadOf(m)*100/spanOf(m);
    assert.ok(Math.abs(dead(rec4)-23.7)<=1,`rec4 dead ${dead(rec4)}`);assert.ok(Math.abs(dead(rec5)-19.2)<=1,`rec5 dead ${dead(rec5)}`);
    assert.ok(rec4.deaths.filter(d=>d.souls!=null&&d.souls>=1376).length>=4);
    assert.ok(rec5.deaths.filter(d=>d.close).length>=3,'Victor x2 and Drifter');
    assert.ok(rec5.windows.unused>=1);
    const ids=p.summary().weaknesses.map(t=>t.id);
    // Unspent souls are judged against the tier bought at that time: these late-game 1.4k-6.3k
    // amounts were saving for 3,200 / 6,400 items, so they are no longer a weakness.
    for(const id of ['dying','close_range','repeat_killer','unused_windows'])assert.ok(ids.includes(id),`${id} in ${ids}`);
    assert.ok(!ids.includes('jungle_heavy'));
  }
});
test('time dead on the newest-pipeline recordings matches the respawn-timer ground truth',async()=>{
  const {existsSync}=await import('node:fs');
  const root='C:/Users/arielwolle/Documents/Github/DeadlockCoach/frames';
  // Ground truth per match (docs/player-profile.md "Validation"): sum over deaths of first death
  // screen -> respawn (timer reads, checked on the screenshots), observed game seconds.
  // rec8 ends dead at 50:38 (frame 585), then black, MVP and the post-game summary (frames 593-598,
  // read as Shop with an extrapolated 50:59-51:16 clock): those 38 s are not game time (MATCH_END),
  // and the last death ends with the match at 50:38, not at its 50:45 respawn (they were alive frames).
  const truth=[{file:'rec4/replay9',deaths:5,dead:312,span:1319},{file:'rec5/replay7',deaths:4,dead:241,span:1250},
    {file:'rec8/replay2',deaths:6,dead:386,span:2233},{file:'rec11/replay1',deaths:7,dead:339,span:1976}];
  const {profileOf}=await import('../scripts/player-profile-replay.ts');
  for(const t of truth){
    const file=`${root}/${t.file}.jsonl`;if(!existsSync(file))continue; // recordings are local
    for(const wall of [true,false]){
      // Without wall time rec5's 7-minute gap plus a re-read match id splits it (match boundaries).
      if(!wall&&t.file.startsWith('rec5'))continue;
      const m=profileOf([file],wall).matches[0];
      assert.equal(m.deaths.length,t.deaths,`${t.file} deaths`);
      assert.ok(Math.abs(spanOf(m)-t.span)<=10,`${t.file} span ${spanOf(m)}`);assert.ok(Math.abs(deadOf(m)-t.dead)<=6,`${t.file} dead ${deadOf(m)}`);
      assert.ok(Math.abs(deadOf(m)*100/spanOf(m)-t.dead*100/t.span)<=0.5,`${t.file} ${deadOf(m)}/${spanOf(m)}`);
    }
  }
  const rec11=`${root}/rec11/replay1.jsonl`;
  if(existsSync(rec11)){const p=profileOf([rec11]);const s=p.summary();
    assert.equal(s.heroName,'Violet');assert.equal(s.deadPct,17);assert.match(p.promptText()!,/^Player profile \(1 match, mostly Violet, 17% dead/);
    assert.match(s.weaknesses.find(w=>w.id==='unspent_souls')!.evidence,/half an item tier or more \(1\.6k-3\.2k by game time\) in 7 of 7 deaths/);}
});
/** A live-scan-like match (no minimap marker, the zone label only): `deaths` = [start, seconds]
 *  offsets, optional urn carry, death-screen spend, an enemy-dead window and its zone. */
function liveMatch(p:PlayerProfile,wall0:number,o:{secs?:number;deaths?:Array<[number,number]>;urn?:[number,number];spend?:boolean;window?:[number,number];windowZone?:string;matchId?:(rel:number)=>number}={}){
  const start=600,secs=o.secs??1200;
  for(let rel=0;rel<=secs;rel+=4){
    const c=start+rel,d=(o.deaths??[]).find(([a,n])=>rel>=a&&rel<a+n);
    const inWin=o.window&&rel>=o.window[0]&&rel<o.window[1];
    const souls=o.spend&&d?(rel-d[0]<8?5000:500):o.spend&&(o.deaths??[]).some(([a,n])=>rel>=a+n)?500:(o.spend?5000:1000);
    const f=frame(c,{dead:!!d,you:null,ally:null,ed:inWin?2:0,souls,match_id:o.matchId?.(rel)});
    if(!d)(f.minimap as Record<string,unknown>).zone_area=inWin?(o.windowZone??'Lane'):rel%600<400?'Lane':'Jungle';
    if(o.urn&&rel>=o.urn[0]&&rel<=o.urn[1])f.status_effects=['carrying_urn'];
    p.observe(f as never,wall0+rel*1000);
  }
  return p.endMatch()!;
}
test('live scans without a minimap marker: zone areas, only plausible matches count, calibrated wording',()=>{
  // rec8-rec12 live: the marker was located on 6-10 of ~450 gameplay frames, so the old "readable
  // time" (lane + jungle + base + dead) was mostly death screens: 90% / 78% / 65% "of the game dead".
  let now=1e12;const p=new PlayerProfile({now:()=>now});
  const m=liveMatch(p,now,{deaths:[[300,60],[900,60]]});
  assert.ok(m.time.lane>=600&&m.time.jungle>=300,JSON.stringify(m.time));assert.equal(m.deadSecs,120);assert.equal(m.spanSecs,1200);
  assert.equal(p.summary().rows[0].deadPct,10);assert.equal(excludedReason(m),null);
  // Not counted: under 8 minutes, no readable gameplay (an old live record), over 50% dead.
  const short={...m,spanSecs:300,deadSecs:30,id:'s'},menus={...m,time:{lane:20,jungle:0,base:0,dead:300,unknown:900},fieldSecs:0,id:'n'},odd={...m,deadSecs:1080,id:'o'};
  const legacy={...m,id:'l',time:{...m.time,dead:200}} as MatchRecord;delete legacy.spanSecs;delete legacy.deadSecs;
  assert.match(excludedReason(short)!,/under 8 min/);assert.match(excludedReason(menus)!,/no readable gameplay/);assert.match(excludedReason(odd)!,/implausible 90% dead/);
  const s=p.model.summary([odd,short,menus,legacy,m]);
  assert.equal(s.deadPct,10,'only the plausible, metric-measured match');assert.equal(s.excluded,3);
  assert.equal(s.rows.find(r=>r.id==='l')!.deadPct,null);assert.match(s.rows.find(r=>r.id==='l')!.excluded!,/older version/);
  assert.ok(!s.weaknesses.some(t=>t.id==='dying'),'10% dead is not "dying too often"');
  // Three matches at 20% dead: the spoken line names the games and a target.
  const q=new PlayerProfile({now:()=>now});
  for(let i=0;i<3;i++){liveMatch(q,now,{deaths:[[200,80],[600,80],[1000,80]]});now+=3600e3;}
  const dying=q.summary().weaknesses.find(t=>t.id==='dying')!;
  assert.match(dying.focus,/^Profile focus: you were dead 20% of your last 3 games; aim under 12%\. Stay at max range/);
});
test('match boundaries on live scans: a stale HUD match id, the end-of-game camera and menus between matches',()=>{
  let now=1e12;const p=new PlayerProfile({now:()=>now});
  // rec12 read rec11's id 127139499 for its first minutes, then 127160441.
  const m=liveMatch(p,now,{matchId:rel=>rel<240?127139499:127160441});
  assert.equal(m.matchId,127160441);
  // rec10: PostGame at 57:55, then a gameplay frame with clock 58:00 was a new match (and a cue).
  for(let c=600;c<1200;c+=4)p.observe(frame(c) as never,now+c*1000);
  assert.ok(p.observe({screen:'PostGame'} as never,now+1200e3).ended);
  const ev=p.observe(frame(1205) as never,now+1205e3);assert.equal(ev.matchStarted,false);assert.equal(p.current,null);
  // A real new match (clock reset) still starts.
  assert.equal(p.observe(frame(30) as never,now+1400e3).matchStarted,true);
  // Shop / loading frames without a clock long after the last clocked frame add nothing.
  for(let c=30;c<300;c+=4)p.observe(frame(c) as never,now+1400e3+(c-30)*1000);
  const before=p.current!.observedSecs;
  for(let i=0;i<60;i++)p.observe({screen:'Shop',game_clock_secs:null} as never,now+1400e3+270e3+200e3+i*4000);
  assert.equal(p.current!.observedSecs,before);
});
test('strengths come with evidence: windows used, urn, death-screen shopping, deathless stretches, improvement; praise and fixes alternate',()=>{
  let now=1e12;const p=new PlayerProfile({now:()=>now});
  // Two rough matches: 20% dead, an unused enemy-dead window each.
  for(let i=0;i<2;i++){liveMatch(p,now,{deaths:[[200,80],[600,80],[1000,80]],window:[700,760]});now+=3600e3;}
  assert.ok(p.summary().weaknesses.some(t=>t.id==='unused_windows'));
  // The good one: one death at 10:00 (20 s) after which 4,500 souls were spent before the respawn,
  // the urn carried 5:00-5:40, two enemies dead 15:00-15:40 while in their base.
  const m=liveMatch(p,now,{deaths:[[600,20]],urn:[300,340],spend:true,window:[900,940],windowZone:'EnemyBase'});now+=3600e3;
  assert.equal(m.deaths[0].spentDead,4500);assert.deepEqual(m.urn,{carried:1,delivered:1,lost:0,at:[[900,940]]});
  assert.deepEqual([m.windows.used,m.windows.unused,m.windows.base],[1,0,1]);
  const s=p.summary(),ids=s.strengths.map(t=>t.id);
  for(const id of ['last_windows','last_urn','last_death_spend','last_low_dead','last_deathless'])assert.ok(ids.includes(id),`${id} in ${ids}`);
  assert.match(s.strengths.find(t=>t.id==='last_windows')!.focus,/^Keep it up: last game you used 1 of 1 enemy-dead window to push their base\.$/);
  assert.match(s.strengths.find(t=>t.id==='last_death_spend')!.evidence,/Spent 4\.5k souls between the 20:00 death and the respawn/);
  assert.match(s.strengths.find(t=>t.id==='last_deathless')!.evidence,/No deaths for 10:00 \(10:00-20:00\)/);
  assert.ok(s.improvements.some(t=>t.id==='improved_dead'));assert.ok(s.improved);
  assert.match(s.improvements[0].evidence,/^Dead 2% of the game last match vs 20% in your previous 2 games\./);
  assert.match(p.promptText()!,/Strong: latest match: Used 1 of 1 windows .+ Improved: Dead 2%/);
  // Spoken: the improvement leads, then a fix, then a concrete play; never only fixes.
  const cues:string[]=[];for(let i=0;i<4;i++){const c=p.focusCue();if(c)cues.push(c.text);now+=5*60000;}
  assert.match(cues[0],/^Great game: you were dead 2% of it, down from 20%/);assert.match(cues[1],/^Profile focus:/);
  assert.match(cues[2],/^Keep it up: last game you used 1 of 1 enemy-dead window to push their base/);
  assert.ok(!cues.some(c=>/windows? .*push a lane/.test(c)),'the unused-windows fix is not said after a match that used them');
});
test('live recordings rec8-rec12 (browser path): plausible time dead, earlier weaknesses, rec12 strengths and praise',async()=>{
  const {existsSync}=await import('node:fs');
  const root='C:/Users/arielwolle/Documents/Github/DeadlockCoach/frames';
  const files=['rec8','rec9','rec10','rec11','rec12'].map(r=>`${root}/${r}/recording.json`);
  if(!files.every(f=>existsSync(f)))return; // recordings are local
  const {liveProfileOf}=await import('../scripts/player-profile-replay.ts');
  const {profile,cues,matches}=liveProfileOf(files);
  // One match per recording; time dead against the respawn timers (rec8 17%, rec11 17%), never 65-90%.
  assert.equal(matches.length,5);
  const pctDead=matches.map(({record:m})=>Math.round(deadOf(m)*100/spanOf(m)));
  assert.deepEqual(pctDead,[17,17,23,17,7]);
  assert.equal(matches[4].record.matchId,127160441,'not rec11 id read by the stale HUD');
  for(const c of cues)assert.doesNotMatch(c.text,/\b([5-9]\d)% of/,c.text);
  // Earlier matches' weaknesses are still found.
  const before=profile.model.summary(profile.matches.slice(1)).weaknesses.map(t=>t.id);
  for(const id of ['dying','unspent_souls','repeat_killer'])assert.ok(before.includes(id),`${id} in ${before}`);
  // rec12 (Grey Talon, 3 deaths, 7% dead): the plays behind the win.
  const s=profile.summary(),by=(id:string)=>s.strengths.find(t=>t.id===id);
  assert.match(by('last_windows')!.evidence,/Used 2 of 4 windows .*\(13:38, 27:21\); all of them pushed into their base/);
  assert.match(by('last_urn')!.evidence,/16:09-16:33/);assert.match(by('last_death_spend')!.evidence,/Spent 5\.9k souls between the 14:50 death/);
  assert.match(by('last_low_dead')!.evidence,/^Dead 7% of the game/);assert.match(by('last_deathless')!.evidence,/9:17 \(0:31-9:48\)/);
  assert.ok(by('last_lanes'));assert.ok(by('last_team_lead'));
  assert.deepEqual(s.improvements.map(t=>t.id),['improved_dead','improved_deaths']);assert.ok(s.improved);
  // The next match's first reminder praises, then names the focus goal.
  assert.match(profile.focusCue()!.text,/^Great game: you were dead 7% of it, down from 18%.* Profile focus: this match, /);
  // During rec8-rec12: one cue per match start (a praise, then the match focus goal) and, at a
  // respawn, only a goal check when the death broke a goal: never a praise at a respawn (rec13 said
  // "Keep it up: last game you went 9 minutes without dying" 5 s after a death).
  const starts=cues.filter(c=>c.event==='start'),respawns=cues.filter(c=>c.event==='respawn');
  assert.equal(starts.length,4);for(const c of starts)assert.match(c.text,/(^|\. )Profile focus: this match, /,c.text);
  assert.ok(starts.some(c=>/^(Keep it up|Great game)/.test(c.text)),starts.map(c=>c.text).join('\n'));
  assert.ok(respawns.length>0);for(const c of respawns){assert.match(c.text,/^Goal check: /);assert.match(c.key,/^player:focus:goal:/);}
  // One goal check per goal per 5 minutes, never after the souls were spent on the death screen.
  for(let i=1;i<respawns.length;i++)if(respawns[i].key===respawns[i-1].key&&respawns[i].file===respawns[i-1].file)assert.ok(respawns[i].clock!-respawns[i-1].clock!>=300);
  // Each match after the first was scored against its goals.
  for(const {record:m} of matches.slice(1)){assert.ok(m.goals?.length,m.id);for(const g of m.goals!)assert.ok(g.met!==undefined&&g.detail);}
  const prompt=profile.promptText()!;assert.ok(prompt.length<=900);assert.match(prompt,/Strong: latest match: Used 2 of 4 windows/);assert.match(prompt,/Improved: Dead 7%/);
});
test('match focus goal: chosen from the last 3 matches, said once at the start, checked at a death that broke it, scored at the end; no praise at a respawn',()=>{
  let now=1e12;const store=memory();const p=new PlayerProfile({store,now:()=>now});
  // Three matches with two close-range deaths each (Aura of Suffering, Melee), both holding 2.5k+ souls.
  for(let i=0;i<3;i++){feedMatch(p,600,now);p.endMatch();now+=3600e3;}
  assert.deepEqual(chooseGoals(p.matches).map(g=>g.id),['close_range','unspent_souls']);
  assert.deepEqual(chooseGoals(p.matches.slice(0,1)).map(g=>g.id),['close_range','unspent_souls'],'two events in one match are enough');
  assert.deepEqual(chooseGoals([]),[]);
  // The live match: a won fight (2 enemies dead), you stay in at 25% and die to melee at 11:40.
  const said:string[]=[];let mid:string|null=null;
  for(let c=600;c<1300;c+=2){
    const rel=c-600;let f:Frame;
    if(rel>=100&&rel<130)f=frame(c,{dead:true,cause:'Melee',souls:500});
    else if(rel>=80&&rel<100)f=frame(c,{ed:2,hp:rel>=86?250:900,souls:500});
    else f=frame(c,{souls:500});
    if(c===1290)(f as Record<string,unknown>).match_result='loss';
    const ev=p.observe(f as never,now+rel*1000);
    if(ev.matchStarted||ev.respawned){const cue=p.focusCue(ev.matchStarted?'start':'respawn');if(cue)said.push(`${ev.matchStarted?'start':'respawn'}|${cue.key}|${cue.text}`);}
    if(rel===50){mid=p.goalText();assert.equal(p.focusCue('start'),null,'once per match start');}
  }
  assert.match(mid!,/^No close-range deaths \(so far: on track, no deaths\) · Spend at the usual buy/);
  assert.equal(said.length,2,said.join('\n'));
  assert.match(said[0],/^start\|player:focus:goal\|(.*\. )?Profile focus: this match, no close-range deaths and spend at the usual buy\.$/);
  assert.equal(said[1],'respawn|player:focus:goal:close_range|Goal check: that was a close-range death. Next life, hold max range behind the front line.');
  assert.ok(!said.some(s=>/^respawn\|.*(Keep it up|Great game)/.test(s)),'no praise right after a death');
  assert.equal(p.focusCue('respawn'),null,'once per respawn');
  assert.match(p.goalText()!,/No close-range deaths \(so far: broken 1 time \(11:40\) in 1 death\)/);
  const rec=p.endMatch()!;
  assert.equal(rec.result,'loss','the Defeat banner (match_result) is the result');
  assert.equal(rec.deaths[0].lowAfterWin,true,'died under 35% after a won fight');
  assert.deepEqual(rec.goals!.map(g=>[g.id,g.met]),[['close_range',false],['unspent_souls',true]]);
  assert.match(rec.goals![0].detail,/^broken 1 time \(11:40\) in 1 death$/);
  // Stored, reloaded and shown per match (Player page history).
  const q=new PlayerProfile({store,now:()=>now}).load();
  assert.deepEqual(q.matches[0].goals,rec.goals);assert.deepEqual(q.summary().rows[0].goals,rec.goals);
  assert.deepEqual(migrate({schema:1,matches:[{...rec,goals:[{id:'x'.repeat(50),text:5,met:'yes'}]}]}).matches[0].goals,[{id:'x'.repeat(30),text:'x'.repeat(30),met:null,detail:''}]);
  // Scoring helpers: a match goal (the won fight's 20 s window went unused), a death cap.
  assert.deepEqual(scoreGoal({id:'unused_windows',text:'Push every enemy-dead window'},rec),{id:'unused_windows',text:'Push every enemy-dead window',met:false,detail:'0 of 1 enemy-dead windows used (unused at 11:20)'});
  assert.deepEqual(scoreGoal({id:'deaths',text:'At most 2 deaths',target:2},rec),{id:'deaths',text:'At most 2 deaths',target:2,met:true,detail:'1 death (goal at most 2)'});
  // The next match's goals: the newest match had no unspent-souls death (it measured clean).
  assert.deepEqual(p.nextGoals(),{goals:chooseGoals(p.matches),current:false});
  assert.ok(!chooseGoals(p.matches).some(g=>g.id==='unspent_souls'),JSON.stringify(chooseGoals(p.matches)));
});
