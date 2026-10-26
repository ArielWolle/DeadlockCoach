import test from 'node:test';
import assert from 'node:assert/strict';
import { defaults, normalize } from '../src/settings.ts';
import { snapshotLines } from '../src/storage.ts';

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
