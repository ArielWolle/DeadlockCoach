import test from 'node:test';
import assert from 'node:assert/strict';
import { defaults, normalize } from '../src/settings.ts';
import { snapshotLines } from '../src/storage.ts';
import { fit, grow, move, normalized, resize, selection, viewport, ViewportError, type Rect } from '../src/crop.ts';
import { consistentBorders, detectBorders } from '../src/borders.ts';
import {FrameHistory} from '../src/frame-history.ts';
import {regionsFromReport} from '../src/region-overlays.ts';

test('settings normalize corrupt input, finite numeric ranges and viewport bounds', () => {
  assert.deepEqual(normalize(null), defaults);
  const s = normalize({ rate: Infinity, volume: -10, interval: 1, crop: { x: .7, y: .7, w: 1, h: 1 }, options: { map: false } });
  assert.equal(s.rate, 1); assert.equal(s.volume, 0); assert.equal(s.interval, 5);
  assert.ok(s.crop.x + s.crop.w <= 1); assert.ok(s.crop.y + s.crop.h <= 1);
  assert.equal(s.options.map, false); assert.equal(s.options.timers, true);
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
