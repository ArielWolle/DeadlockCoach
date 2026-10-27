"""Extract the screenshots of an exported recording into NNN_<timestamp>.jpg files for replay.
Usage: extract.py deadlock-recording.json OUT_DIR"""
import base64, json, os, sys
d = json.load(open(sys.argv[1], encoding='utf-8'))
os.makedirs(sys.argv[2], exist_ok=True)
for i, r in enumerate(d['frames']):
    open(os.path.join(sys.argv[2], f'{i:03d}_{r["timestamp"]}.jpg'), 'wb').write(base64.b64decode(r['image'].split(',', 1)[1]))
print(len(d['frames']), 'frames;', len(d.get('spoken', [])), 'spoken cues in the export')
