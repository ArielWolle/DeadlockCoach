# Match Timeline Cheat Sheet (in-game clock, seconds)

Sources (all retrieved 2026-10-25; see the topic files for per-page last-modified dates):
- https://deadlock.wiki/Cursed_Apple (Map; last modified 2026-10-25; page tagged "outdated, timings should be correct but worth double checking")
- https://deadlock.wiki/Strategy (last modified 2026-09-14, partly outdated)
- https://deadlock.wiki/Midboss (2026-10-12), /Soul_Urn (2026-10-23), /Unstable_Rift (2026-09-28), /Sinner's_Sacrifice (2026-10-22), /Trooper (2026-10-23), /Powerup (2026-08-24), /Zipline (2026-10-04), /Lane_Guardian (2026-10-04), /Walker (2026-10-13), /Souls (2026-10-25), /Boon (modified date not captured)
Game patch/date basis: latest wiki-listed gameplay changes through 2026-10-13 (Soul Urn voice filter) / 2026-10-05 (trooper walking bug fix).

Anything marked UNVERIFIED was not found on a fetched page or the pages disagree.

## Quick table
| clock | secs | event |
|---|---|---|
| 0:00 | 0 | Match start. Midboss already spawned (since 2026-05-22). Every player has 600 souls. Guardian resist 50%, Walker resist 65% (both start decaying). Tier 1 permanent buffs. |
| first wave | UNVERIFIED | First trooper wave spawn second not stated on wiki. |
| 2:00 | 120 | Easy Haunt camps spawn (4); respawn 85 s after full clear. |
| 2:30 | 150 | Healing Snacks (Cursed Apple page: respawn 3 min after pickup). Soul Well starts releasing 10-soul orbs every 2 s per Souls page (150 s) BUT Cursed Apple spawn-room text says 3 min (180 s): UNVERIFIED. |
| 3:00 | 180 | Most breakables first spawn (crates, Golden Statues); respawn 180 s after broken. Soul Well per Cursed Apple page. |
| 5:00 | 300 | Melee Troopers join waves (waves become 2 Ranged + 1 Melee + 1 Medic). Powerups (Bridge Buffs, 2 locations) spawn, then every 300 s, last 160 s. Medium Haunt camps (22) spawn; respawn 290 s after clear. Tunnel breakables / Sunken Plaza / Bell Tower breakables: respawn 300 s / 180 s. |
| 6:00 | 360 | Zipline Speed Boost unlocks (+80%, 32 s, cooldown 360 s). Lane Guardian resist reaches 0% (50% -> -50% over first 720 s). Enemy kills here let enemies return fast per Strategy page. |
| 8:00 | 480 | Sinner's Sacrifices (15 at 11 locations) spawn; respawn 300 s. Hard Haunt camps (11) spawn; respawn 335 s. Laning phase ends per catch-up mechanic (lowest two players receive passive souls after 480 s). |
| 9:00 | 540 | Walker resist reaches 0% (65% -> -65% over 1080 s). |
| 10:00 | 600 | Soul Urn spawns (Yellow first), lands 612.5 s; every 300 s afterward (900, 1200, ...). Permanent buff Tier 2 begins. Mid boxes (breakables above Midboss) spawn (180 s respawn). Earliest possible Unstable Rift in Cursed Apple model. |
| 11:00 +/- 60 s | 660 +/- 60 | Unstable Rift in-world visual effect appears on a random side-lane bridge (Unstable Rift page). ~60 s later global announcement; 20 s after that the Rift opens and capture begins (Unstable Rift page). Cursed Apple page says 40 s to announcement: pages disagree - UNVERIFIED. |
| 12:00 | 720 | Lane Guardians reach their minimum resist (-50% = take 50% extra). |
| 13:00 | 780 | Unstable Rift bounty scaling starts (1482 + 222 per min from minute 13). |
| after rift | +420 +/- 60 | Next Rifts every 7 min (6 to 8 min window). Strategy page still says 8 min: outdated. |
| 15:00 | 900 | Soul Urn #2 (Green lane). Powerup respawn interval. |
| 18:00 | 1080 | Walkers reach -65% resist (take 65% extra). Medic pack pickup range maxes at 30 m. |
| 20:00 | 1200 | Waves every 25 s. Patron phase 1 max HP starts +250/min. Soul Urn #3. |
| 25:00 | 1500 | Troopers at 20% spirit resist. Soul Urn #4. |
| 30:00 | 1800 | Permanent buff Tier 3 begins. |
| 35:00 | 2100 | Waves every 20 s. Troopers gain +50% HP. |
| 40:00 | 2400 | Hero kill bounty hits cap 2200 souls (200 + 50/min). Powerups reach full strength. |

## Midboss schedule (see midboss.md)
- Alive from 0 s. After 1st kill: respawn +420 s; 2nd: +360 s; 3rd+: +300 s.
- Kill -> Rejuvenator lands +6 s -> 3 credits max -> buff 180 s; revive delay 3 s.

## Objective windows by minute (planning aid)
- 0-5 min: laning, no cheap neutral jungle beyond easy camps (from 120 s). Midboss is available but untouched in practice (UNVERIFIED how early teams can safely kill it; no benchmark on wiki).
- 5-8 min: Powerups, medium camps, melee troopers; Zipline Boost at 360 s.
- 8-10 min: Sinners and hard camps; first urn at 600 s.
- 10-13 min: first Rift window (660 +/- 60 s start); guardians at -50% by 720 s so first guardian pushes are strongest from 12:00.
- 18+ min: Walkers at -65%; Walker kills give Extra Slots and 3675 souls.
- 20+ min: waves every 25 s; Patron HP scaling begins.

## UNVERIFIED / missing
- First trooper wave spawn second, zipline travel time, minute-by-minute souls/min or net-worth benchmarks, expected item purchase times (see souls-and-power-curve.md).
- Soul Well start (150 vs 180 s).
- Rift announcement lead time (40 vs 60 s).

## Machine-friendly table
| name | first_spawn_secs | interval_secs | notes |
|---|---|---|---|
| midboss_alive | 0 | 420/360/300 | respawn 7/6/5 min after death |
| easy_camps | 120 | 85 | 4 camps; respawn after full clear |
| soul_well | 150 or 180 UNVERIFIED | 2 | 10-soul orb |
| breakables | 180 | 180 | tunnels 300 |
| melee_trooper | 300 | n/a | join waves |
| powerups | 300 | 300 | last 160 s |
| medium_camps | 300 | 290 | 22 camps |
| zipline_boost | 360 | 360 | 32 s duration |
| sinners_sacrifice | 480 | 300 | 15 machines |
| hard_camps | 480 | 335 | 11 camps |
| soul_urn | 600 | 300 | lands 612.5 s |
| unstable_rift | 660 +/- 60 (effect) | 420 +/- 60 | ann. ~60 s later, opens ~20 s after |
| guardian_resist_min | 720 | n/a | -50% |
| walker_resist_min | 1080 | n/a | -65% |
| wave_25s | 1200 | 25 | |
| perm_buff_tier3 | 1800 | n/a | |
| wave_20s | 2100 | 20 | troopers +50% HP |
