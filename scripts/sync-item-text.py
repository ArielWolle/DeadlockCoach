"""Adds the in-game tooltip text to data/items.json: `desc` (the item's passive/active sentence,
HTML stripped) and `effect` (its headline values, e.g. "+40% Weapon Damage beyond Range").
Source: api.deadlock-api.com/v1/assets/items (the game's own localized tooltips). Re-run after
patches; sync-assets.ps1 calls it."""
import json, os, re, sys, urllib.request

root = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
path = os.path.join(root, 'data', 'items.json')
req = urllib.request.Request('https://api.deadlock-api.com/v1/assets/items', headers={'User-Agent': 'DeadlockCoach-sync'})
api = {i['class_name']: i for i in json.load(urllib.request.urlopen(req, timeout=60))}

def clean(s):
    s = re.sub(r'<[^>]+>', ' ', s or '')
    s = re.sub(r'\{[^}]*\}', '', s)  # unresolved template tokens
    s = re.sub(r'\s+', ' ', s).strip()
    s = re.sub(r'\s+([.,;:!?])', r'\1', s)
    return re.sub(r'([.!?])(?=[A-Z])', r'\1 ', s)

def value(p):
    v = str(p.get('value', '')).strip()
    if not v or v in ('0', '-1', '0.0'): return None
    if re.fullmatch(r'-?\d+(\.\d+)?', v):
        n = float(v); v = ('+' if n > 0 and '{s:sign}' in p.get('prefix', '') else '') + (str(int(n)) if n.is_integer() else v)
    elif '{s:sign}' in p.get('prefix', '') and not v.startswith('-'): v = '+' + v
    post = p.get('postfix', '')
    if post and not v.endswith(post): v += post
    out = f"{v} {p.get('label', '').strip()}".strip()
    if p.get('conditional'): out += f" {p['conditional']}"
    return out

def describe(item):
    descs, effects = [], []
    for sec in item.get('tooltip_sections') or []:
        for a in sec.get('section_attributes') or []:
            if a.get('loc_string'): descs.append(clean(a['loc_string']))
            names = lambda k: a.get(k).split() if isinstance(a.get(k), str) else list(a.get(k) or [])
            for name in names('important_properties') + names('elevated_properties'):
                p = (item.get('properties') or {}).get(name)
                v = p and value(p)
                if v and v not in effects: effects.append(v)
    if not descs and (item.get('description') or {}).get('desc'): descs.append(clean(item['description']['desc']))
    return ' '.join(d for d in descs if d), '; '.join(effects[:3])

items = json.load(open(path, encoding='utf-8-sig'))
missing = 0
for it in items:
    a = api.get(it['class_name'])
    if not a: missing += 1; continue
    it['desc'], it['effect'] = describe(a)
json.dump(items, open(path, 'w', encoding='utf-8'), indent=2, ensure_ascii=False)
open(path, 'a', encoding='utf-8').write('\n')
print(f"{len(items) - missing} items described, {missing} not in the API", file=sys.stderr)
