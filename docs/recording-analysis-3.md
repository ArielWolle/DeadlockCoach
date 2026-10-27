# Recorded match analysis 3

A 1920x1080 match (Baba, Yellow lane), 383 frames, game clock 0:24-10:40, 50 spoken cues,
recorded on the build from analysis 2. User notes: lane tracking often wrong; "go in" when enemies
just overlap; item suggestions too general. Two independent reviewers: one rated every tip from the
exported data alone, one checked the minimap calls against the screenshots.

Data-only rating: 30% helpful, 26% generic, 6% mistimed, 12% unsupported, 26% wrong. Timers were
almost all right; 10 of 18 positioning calls (help / join / outnumbered / go in / gank) were wrong.
The screenshot audit found the cause of all 9 wrong fight calls it checked: **portraits overlapping
yours**. The tracker still held those heroes 0.02-0.07 away, unseen for 1-11 s, but the advice
counted only rings drawn in the current frame.

| Measure (replay of the same 383 frames) | Before | After |
| --- | --- | --- |
| Positioning calls spoken | 20 | 7 |
| "Teammate in trouble: 1 against 1" | 4 (6 live) | 0 |
| Audited wrong calls still made (frames 46/72 outnumbered, 136 go in) | 3 | 0 |
| Gameplay frames with your lane / your position | 205 / 236 of 300 | 242 / 258 |
| Lane tip naming the wrong lane ("Green" after respawn) | 1 | 0 |
| False "Midboss is back" (8 s icon flicker at 1:23) | 1 | 0 |
| Impossible max HP (13572 for 1357) | 1 | 0 |
| Death at 8:10: calls before it | back off 53%, last hit 38%, gank 24%, critical 1% | back off 38%, critical 24% |

rec2 replay (regression check): lane known in 357 gameplay frames (was 277), no new clock jumps.

## Changes

- **Heroes under your portrait count.** A confirmed track seen in the last 3 s, or last seen within
  0.08 of you in the last 10 s, is present. A teammate under you still cancels "outnumbered" / gank
  for 15 s; commit calls use the stricter count. Hidden enemies are not "missing".
- **Fight calls need two consistent reads** (gank excepted), and a commit call never follows a back-off
  call within 30 s, or the reverse.
- **Help / join:** an even fight is not "trouble"; help needs more enemies than allies, enemies are
  counted in a 1.6x wider circle than allies so an arriving group is part of the fight.
- **Danger first:** under 30% health, or under 45% and falling, only High-priority calls speak.
  "Critical" also fires under 30% when health falls more than 5 points per read, and wins ties.
- **Lane:** the lane gate is 24 px (standing by your guardian measured 19-23 px; jungle 27+), the lane
  is held for 8 s while you stay within 0.05 of where it was confirmed, your marker must be on the map
  disc, and it is confirmed against any sighting in the last 10 s (one miss used to cost two frames).
  A teammate 0.017 from you is no longer deleted as your own ring.
- **Midboss:** an absence shorter than the shortest respawn (5 min) was a covered icon, not a kill.
- **Max HP** may not jump +-40% between reads unless three identical reads agree.
- **Item tips** now say what the item does, its cost and the evidence, e.g. "Against Viscous, consider
  Long Range for 1600 souls. Deal additional Weapon Damage when beyond a minimum distance from your
  target: +40% Weapon Damage beyond Range. In recorded matches, players who bought it won 4.5
  percentage points more often against Viscous." The text is the game's own tooltip
  (`scripts/sync-item-text.py` adds `desc` / `effect` to `data/items.json` from the Deadlock API).
  A damage item of the category you own fewer of is not suggested, and in a match a tip waits for a
  shop window (dead, scoreboard, or back at base).

## Known limits

- Two portraits stacked exactly on each other (not on you) still read as one ring.
- Counter statistics are win-rate differences, not a mechanism; the coach says so.
- Several items share one icon (Extra Regen / Mystic Regeneration), so owned-item checks can be wrong.
- Hero ids per minimap ring are still not known, so calls cannot name the enemy.
