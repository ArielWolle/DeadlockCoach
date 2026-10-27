"""Contact sheet of whole frames (downscaled). Usage: thumbs.py IMG_DIR OUT.png WIDTH idx..."""
import sys, os, glob
from PIL import Image, ImageDraw
files = sorted(glob.glob(os.path.join(sys.argv[1], '*.jpg')))
w = int(sys.argv[3]); idx = [int(a) for a in sys.argv[4:]]
tiles = []
for i in idx:
    im = Image.open(files[i]).convert('RGB'); h = im.height * w // im.width
    t = im.resize((w, h)); ImageDraw.Draw(t).text((4, 4), str(i), fill=(255, 255, 0)); tiles.append(t)
cols = max(1, min(len(tiles), 1800 // w)); rows = (len(tiles) + cols - 1) // cols
sheet = Image.new('RGB', (cols * w, rows * tiles[0].height))
for n, t in enumerate(tiles): sheet.paste(t, ((n % cols) * w, (n // cols) * t.height))
sheet.save(sys.argv[2]); print(sys.argv[2], sheet.size)
