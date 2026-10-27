"""Before/after summary. Usage: compare.py deadlock-recording.json replay.jsonl"""
import json, sys, os
here = os.path.dirname(os.path.abspath(__file__))
live = json.load(open(sys.argv[1], encoding='utf-8'))['frames']
rep = [json.loads(l) for l in open(sys.argv[2], encoding='utf-8')]
S = lambda r: r['snapshot']
def flips(rows):
    tot = dead = 0
    for i in range(12):
        seq = ''
        for r in rows:
            hs = S(r).get('hero_status') or []
            if len(hs) != 12: continue
            h = hs[i]
            seq += 'D' if h.get('dead') else 'U' if h.get('bar_visible') is False else 'H' if h.get('health_ratio') is not None else '?'
        tot += sum(1 for a, b in zip(seq, seq[1:]) if a != b and a in 'HU' and b in 'HU'); dead += seq.count('D')
    return tot, dead
for name, rows in (('live', live), ('fixed', rep)):
    hp = [(S(r).get('hp'), S(r).get('max_hp')) for r in rows]
    low = sum(1 for h, m in hp if h and m and h / m < 0.45)
    mm = lambda r: S(r).get('minimap') or {}
    pr = {l: [mm(r).get(l, {}).get('push') for r in rows if mm(r).get(l, {}).get('push') is not None] for l in ('green', 'blue', 'yellow')}
    you = sum(1 for r in rows if mm(r).get('you'))
    bad = sum(1 for r in rows if (S(r).get('souls') or 0) > 2000)
    f, d = flips(rows)
    game = sum(1 for r in rows if S(r).get('screen') == 'Gameplay')
    print(f"{name:5}: gameplay {game}/120 | HP read {sum(1 for h, _ in hp if h)} (<45%: {low}) | your marker {you} | "
          f"push G {min(pr['green']):.2f}-{max(pr['green']):.2f} Y {min(pr['yellow']):.2f}-{max(pr['yellow']):.2f} | "
          f"seen/unseen flips {f} | dead slot-frames {d} | souls>2000 {bad}")
