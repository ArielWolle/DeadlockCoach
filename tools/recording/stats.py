import json, collections, base64, os, statistics as st
here = os.path.dirname(os.path.abspath(__file__))
import sys
src = sys.argv[1]
if src.endswith('.jsonl'):
    rows = [json.loads(l) for l in open(src, encoding='utf-8')]
    for r in rows: r['timestamp'] = r['t']
else:
    rows = json.load(open(src, encoding='utf-8'))['frames']
t0 = rows[0]['timestamp']
print('== mean scan ms', round(sum(r.get('ms', 0) for r in rows) / len(rows)), ' advice', collections.Counter(r.get('advice') for r in rows).most_common(), ' lane', collections.Counter(str(r.get('lane')) for r in rows).most_common(6))
print('== cues spoken')
cues = collections.Counter()
for r in rows:
    for c in r.get('cues') or []: cues[c['key']] += 1
for k, v in cues.most_common(): print(f'  {v:3d}  {k}')
print('\n== cue timeline')
for r in rows:
    for c in r.get('cues') or []:
        s = r['snapshot']; print(f"  t+{(r['timestamp']-t0)/1000:6.1f} clock {s.get('game_clock_secs')} hp {s.get('hp')}/{s.get('max_hp')} d={s.get('health_delta_ratio')} obs={s.get('health_observations')} :: {c['key']} :: {c['text'][:90]}")
print('\n== own health')
hp = [(r['snapshot'].get('hp'), r['snapshot'].get('max_hp')) for r in rows]
ratios = [h / m for h, m in hp if h and m]
print('  read', len(ratios), '/', len(rows), ' min', round(min(ratios), 2), ' median', round(st.median(ratios), 2))
print('  frames < 45%:', sum(r < 0.45 for r in ratios), ' delta < -8%:', sum(1 for r in rows if (r['snapshot'].get('health_delta_ratio') or 0) < -0.08))
print('\n== screen', collections.Counter(r['snapshot'].get('screen') for r in rows))
print('\n== top bar per slot: states per frame (H=hp seen, U=no bar/unseen, D=dead, ?=unknown, -=no data)')
flips = []
for i in range(12):
    seq = ''
    for r in rows:
        hs = r['snapshot'].get('hero_status') or []
        if len(hs) != 12: seq += '-'; continue
        h = hs[i]
        seq += 'D' if h.get('dead') else 'U' if h.get('bar_visible') is False else 'H' if h.get('health_ratio') is not None else '?'
    f = sum(1 for a, b in zip(seq, seq[1:]) if a != b and a in 'HU' and b in 'HU')
    flips.append(f)
    print(f"  {'A' if i<6 else 'E'}{i%6} flips={f:3d} {seq}")
print('\n== enemy health jitter (abs frame-to-frame change when both read)')
for i in range(6, 12):
    vals = [((r['snapshot'].get('hero_status') or [{}]*12)[i] or {}).get('health_ratio') for r in rows]
    dl = [abs(a - b) for a, b in zip(vals, vals[1:]) if a is not None and b is not None]
    if dl: print(f'  E{i-6}: n={len(dl)} median {st.median(dl):.2f} p90 {sorted(dl)[int(len(dl)*0.9)]:.2f} max {max(dl):.2f}')
print('\n== minimap')
def m(r): return r['snapshot'].get('minimap') or {}
print('  observations', collections.Counter(m(r).get('observations') for r in rows))
for lane in ['green', 'blue', 'yellow']:
    push = [m(r).get(lane, {}).get('push') for r in rows]
    at = [m(r).get(lane, {}).get('ally_troopers') for r in rows]; et = [m(r).get(lane, {}).get('enemy_troopers') for r in rows]
    pr = [p for p in push if p is not None]
    print(f"  {lane:6s} push read {len(pr)}/{len(rows)} range {min(pr) if pr else None}-{max(pr) if pr else None} | ally dots {collections.Counter(at).most_common(6)} | enemy dots {collections.Counter(et).most_common(6)}")
print('  you seen', sum(1 for r in rows if m(r).get('you')), ' lane', collections.Counter(m(r).get('your_lane') for r in rows))
vis = [(sum(1 for h in m(r).get('heroes', []) if h['ally'] and h['unseen_ms'] == 0), sum(1 for h in m(r).get('heroes', []) if not h['ally'] and h['unseen_ms'] == 0)) for r in rows]
print('  visible allies/enemies per frame', collections.Counter(vis).most_common(8))
