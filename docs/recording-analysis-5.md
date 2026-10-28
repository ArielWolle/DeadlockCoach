# Recorded match analysis 5: second Grey Talon review, and grading the death reviews

Grey Talon, 600 frames covering game clock 16:33-44:26 (nothing recorded 25:31-32:57), a loss. The
first match recorded with the review coach from analysis 4. Two independent reviewers again went
through the screenshots (deaths; macro play) and graded the coach's own explanations.

## The match

| Death | What happened (screenshots) | Lesson |
| --- | --- | --- |
| ~24:59 | 25:00 objective fight; stunned twice inside Victor's Aura of Suffering (x28), knocked off the zipline, Artist's Mystic Shot finished; 100% -> 0 in ~8-10 s | Back out to bow range when the Aura badge appears; don't zipline into the fight |
| ~32:52 | (gap in recording) Aura of Suffering x29 again, fighting inside the enemy wave, Paradox's Pulse Grenade; 1,229 souls unspent | Second death to Victor: itemise against him, never let him reach you |
| ~38:40 | Went back out at 44-49% health after shopping 6,300 souls, cast into the wave out of stamina; Drifter and the Artist; 1,546 -> 58 in ~6 s | Heal to full first; shopping does not heal |
| 41:03 | 57 s after respawning, farming the left lane; Stalker's Mark on you for 4-6 s, silenced, knocked off the zipline; solo kill by Drifter; 3,467 souls unspent | When marked, stand by a teammate or wall, get off the zipline, save the escape |

- Compared with the previous match: time dead dropped from about 25% to 17-21%, and only one of the
  clearly recorded deaths was alone. Still the same: every death in melee range (Victor's Aura,
  Drifter), hard crowd control before each death, unspent souls (peaks of 5.3k and 6.3k, about 43%
  of alive time over 3,000), and enemy-dead windows not used (37:24-38:38, two enemies dead for 74 s).
- Jungle again low (about 11%); 7 of 12 fights joined, 2 late, 3 missed (17:12 alone on the enemy
  half, 42:43 and 43:17 pushing alone while the base was attacked).

## Grading the coach, and fixes

| Coach output | Grade | Fix |
| --- | --- | --- |
| 25:00 "69% to dead in 2 s, damage panel: Still Life" | partly | The fall is measured from the healthiest reading in the window (now "100% to dead in 15 s"); the panel is re-read every 5 s while dead and the latest read wins (it fills in over several seconds) |
| 32:57 "no clear cause" | partly | Nothing was recorded before it: now "No screenshots were recorded in the seconds before this death"; a death screen more than 60 s after the last one is a new death (it had been merged) |
| 38:49 "no clear cause" | wrong | Bursts count from 30% (now "48% to dead in 12 s") and a new reason: "You were under 50% health for the 30 seconds before. Heal to full before you go back in; shopping does not heal you." |
| 41:03 "100% to dead in 6 s, 3,467 souls" | mostly right | Adds "You died 57 seconds after respawning." |
| Pattern "wave sat deep while you were away", said while dead and repeated | C-/D | Base time is not "away" (defending); the same summary is never said twice; deaths in the window count death-screen runs |
| Missing: unused enemy-dead windows | - | "At 38:21, 2 enemies were dead for 22 seconds and you stayed on your half. Push a lane or take an objective while they are down." |
| "Your team is ahead in souls" at -20k / -29k | wrong | A fresh ally total was compared with a 7-minute-old enemy total after the recording gap, and a +43% misread was inside the band. Totals now expire after 60 s, the band is +8% per read, an out-of-band rise needs three reads, and the call needs four stable readings |

Replay of the same frames afterwards: one review per death (four), the gap stated, no false lead.

## Not yet detected (reviewer suggestions)

- Status text near the crosshair (STUNNED, SILENCED, OUT OF STAMINA, "Knocked off Zipline") and debuff
  badges (Aura of Suffering, Stalker's Mark on you) as death reasons and live warnings.
- The damage panel's Killing Blow row, per-hero damage share and the killer's name.
- Souls on the death screen; recording gaps in the export itself.
