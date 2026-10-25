# Troopers and Waves

Sources (retrieved 2026-10-25):
- https://deadlock.wiki/Trooper (page last modified 2026-10-23; latest update entry 2026-10-05)
- https://deadlock.wiki/Souls (last modified 2026-10-25)
- https://deadlock.wiki/Cursed_Apple (Map; last modified 2026-10-25; page flagged "outdated" by wiki editors)
- https://deadlock.wiki/Unstable_Rift (last modified 2026-09-28)
- https://deadlock.wiki/Shrine (last modified 2026-10-20)

## Wave spawn interval
| match time | interval |
|---|---|
| 0 to 20 min (0-1200 s) | 30 s |
| 20 to 35 min (1200-2100 s) | 25 s |
| after 35 min (2100+ s) | 20 s |

- Cursed Apple page timeline agrees: 20:00 -> every 25 s; 35:00 -> every 20 s, and Troopers gain +50% health at 35:00.
- First wave spawn time (seconds on clock): UNVERIFIED - not stated on any fetched page. Do not assume.
- Waves arrive by ziplines in a vessel (can be shot down to drop early).

## Composition
- Initially 3 Ranged + 1 Medic. After 5 minutes (300 s): 2 Ranged + 1 Melee + 1 Medic (4 per wave). (Melee Troopers only start spawning after 5 minutes.)

## Unit stats (Trooper page)
| type | HP | notes |
|---|---|---|
| Ranged | 300 | DPS 28, range 25 m, move 6.3 m/s |
| Melee | 400 | melee dmg 80, range 3 m, move 8.1 m/s |
| Medic | 350 | DPS 28, range 36 m, heals via Medic Pack |
- Spirit resist 0% growing to 20% at 25 min and 70% at 50 min. +35% damage resistance near/in enemy base. +50% HP at 35 min.
- Health regen out of combat 7. Sight range vs NPCs 41 m, vs heroes 25 m.
- Trooper damage output vs: players 28 dps, troopers 35, guardians 14.4, walkers 24, base guardians 80, shrines 50, patron 50.

## Bounty (souls)
- Trooper bounty = 100 + 2 per minute (rescaled 2026-06-30 from 116 + 1.16/min). Examples (computed): 0 min 100; 10 min 120; 20 min 140; 30 min 160.
- 50% ground orb / 50% flying deniable orb (flying orb ratio raised from 40% to 50% on 2026-06-30). Melee kill: no flying orb, 100% on kill.
- Ground orb lasts 18 s (40 s after 10 min).
- Share among nearby allies: 1 player 100%; 2 players 54% each; 3 -> 36%; 4 -> 25%; 5 -> 20%; 6 -> 16%.
- Sharing radius: 45 m from the orb when it breaks, 30 m from the killing hero.
- Medic Pack: heal = 13% of missing HP + (150 + 2/min)/(nearby heroes); pickup range 18 m at 10 min growing to 30 m at 18 min; cannot be picked up by trooper's own team.
- Parrying a trooper stuns it and does not reset parry cooldown (2026-05-28).

## Super Troopers
- Once an enemy Shrine is destroyed, allied troopers from that shrine's lane (and Blue lane once either shrine is destroyed) become Super Troopers for the rest of the match: +60% damage, +40% health, yield 15% less bounty when killed.

## Rift Troopers (Unstable Rift winner)
- Black, larger, drop no souls. Min 5 spawn (one every 0.5 s); trailing team capturing gets up to 9 extra (14 total).
- First rift: +100% HP & damage, 25% melee resist, 30% spirit resist; each subsequent rift +20% (cap 160%) and +5% spirit resist (cap 45%).

## Related timings (Cursed Apple / Strategy)
- Zipline Boost unlocks at 6:00 (360 s): +80% speed, 32 s duration, 360 s cooldown (https://deadlock.wiki/Zipline).
- Soul Well starts releasing 10-soul orbs every 2 s after 2:30 (Souls page) - Cursed Apple page says 3:00 (180 s) ("activates 3 minutes into the match"). CONFLICT: 150 vs 180; treat as UNVERIFIED which is current.

## UNVERIFIED
- First wave spawn second; time for wave to reach each lane Guardian.
- Wave composition after 20 or 35 min beyond the stat scaling above.

## Machine-friendly table
| name | first_spawn_secs | interval_secs | notes |
|---|---|---|---|
| trooper_wave_phase1 | UNVERIFIED | 30 | 0-1200 s; 3R+1M then (300 s+) 2R+1Melee+1M |
| trooper_wave_phase2 | 1200 | 25 | 1200-2100 s |
| trooper_wave_phase3 | 2100 | 20 | after 2100 s; troopers +50% HP at 2100 s |
| melee_trooper_start | 300 | n/a | melee trooper added to waves |
| trooper_bounty | 0 | n/a | 100 + 2/min, 50% ground / 50% flying orb |
| zipline_boost_unlock | 360 | 360 | cooldown 360 s; duration 32 s |
