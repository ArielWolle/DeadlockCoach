"""Write data/abilities.json: damage-source names (hero abilities, shop items, weapons) from the
public Deadlock API, used to normalise OCR'd death-panel rows and status badges.
Usage: python scripts/gen-abilities.py   (re-run after game patches)"""
import json, os, urllib.request

API = "https://api.deadlock-api.com/v1/assets"
# Sources shown in the damage panel that are not API items (neutral/world damage).
EXTRA = ["Gun Damage", "Melee", "Knocked off Zipline", "Trooper", "Guardian", "Walker", "T1 Guardian",
         "T2 Walker", "Temple Guardian", "Base Guardian", "Shrine", "Patron", "Mid Boss", "Fall Damage",
         "Sinner's Sacrifice", "Neutral"]


def get(path):
    rq = urllib.request.Request(f"{API}/{path}", headers={"User-Agent": "DeadlockCoach/0.1"})
    return json.load(urllib.request.urlopen(rq, timeout=60))


def main():
    root = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
    heroes = {h["id"]: h["class_name"] for h in get("heroes")}
    out, seen = [], set()
    for i in get("items"):
        name = (i.get("name") or "").strip()
        kind = {"ability": "ability", "upgrade": "item", "weapon": "weapon"}.get(i.get("type"))
        # Internal names ("citadel_...", "Ability_...") are never shown in the panel.
        if not kind or not any(c.isalpha() for c in name) or "_" in name or name.lower() in seen:
            continue
        if kind == "upgrade" and not i.get("shopable"):
            continue
        seen.add(name.lower())
        row = {"name": name, "kind": kind}
        hs = [heroes[h] for h in i.get("heroes") or [] if h in heroes]
        if hs:
            row["heroes"] = hs
        out.append(row)
    for n in EXTRA:
        if n.lower() not in seen:
            seen.add(n.lower())
            out.append({"name": n, "kind": "world"})
    out.sort(key=lambda r: r["name"].lower())
    with open(os.path.join(root, "data", "abilities.json"), "w", encoding="utf-8") as f:
        json.dump(out, f, indent=1, ensure_ascii=False)
        f.write("\n")
    print(len(out), "names")


if __name__ == "__main__":
    main()
