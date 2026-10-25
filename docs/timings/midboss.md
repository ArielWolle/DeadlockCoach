# Midboss (Mid-Boss / Temple Guardian)

Source: https://deadlock.wiki/Midboss (redirect from /Mid-Boss)
Retrieved: 2026-10-25
Page last modified: 2026-10-12. Latest update-history entry on page: 2026-05-22. Cross-check: https://deadlock.wiki/Strategy and https://deadlock.wiki/Cursed_Apple (Map).

## Spawn / respawn
- Starts spawned at the beginning of the game (change 2026-05-22). Spawn is underground in the sewers under Broadway (middle lane).
- Strategy page also lists "spawns at 0:00" (https://deadlock.wiki/Strategy).
- Respawn after death: 1st death -> 420 s (7 min); 2nd death -> 360 s (6 min); 3rd and later deaths -> 300 s (5 min).
- Respawn is announced by a roar plus herald "Midboss has arrived". Minimap icon fills as a timer; Tab shows exact time.
- NOTE: Strategy page says "Midboss spawns every 6 minutes" - that page is older (last modified 2026-09-14, oldid 157579) and conflicts with the Midboss page; treat Midboss page (7/6/5) as authoritative.

## Stats
- Base health 13000; regen 15 HP/s; Bullet Resist 15%; Spirit Resist 0%.
- Health scaling: +195 HP per minute after it spawns (linear).
- Damage 20, DPS 100, range 38 m (beam on closest player in pit).
- Shield: starts at 35 HP and increases 5 HP/min; constantly regenerating; health damage only after shield depleted.
- Debuff Resistance 35% (2026-03-06 change).
- Only damageable by players inside its pit; invulnerable to outside attacks; dormant until a player enters the pit.
- Roars at 50% health (global announcement threshold).

## Reward on kill
- Killing team gets 3000 souls (+50 per minute) - split equally among the team (per https://deadlock.wiki/Souls table).
- 2 permanent buffs per player.
- Rejuvenator crystal spawns above pit and descends for 6 s; not claimable until landed. See rejuvenator.md.
- Either team may take the Rejuvenator, even the team that did not kill Midboss.

## Update history (from page)
- 2026-05-22: starts spawned at game start.
- 2026-03-06: base bounty 2000 -> 3000; base HP 11900 -> 13000; announce threshold 70% -> 50%; 35% Debuff Resistance.
- 2025-12-03: Walkers and Midboss +15% Bullet Resistance and -15% HP.

## UNVERIFIED
- Exact in-game clock at which the first Midboss can be killed in practice (page says it starts spawned; no clock second beyond 0 given).
- Whether the 35 HP shield / 5 HP per minute value is current (stated on page; not cross-checked).

## Machine-friendly table
| name | first_spawn_secs | interval_secs | notes |
|---|---|---|---|
| midboss_respawn_1 | 0 | 420 | first spawn at game start; 7 min after 1st death |
| midboss_respawn_2 | n/a | 360 | 6 min after 2nd death |
| midboss_respawn_3plus | n/a | 300 | 5 min after 3rd+ death |
| midboss_kill_bounty | 0 | n/a | 3000 + 50/min souls, team split |
| midboss_health | 0 | n/a | 13000 + 195/min |
| rejuv_crystal_descent | n/a | 6 | seconds after kill until claimable |
