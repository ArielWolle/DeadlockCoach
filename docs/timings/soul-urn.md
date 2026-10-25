# Soul Urn

Source: https://deadlock.wiki/Soul_Urn ; also https://deadlock.wiki/Cursed_Apple (map timings), https://deadlock.wiki/Strategy
Retrieved: 2026-10-25
Page last modified: 2026-10-23. Latest update-history entry: 2026-10-13 (voice filter, undocumented); last gameplay change listed: 2026-07-09.

## Spawn
- First spawn at 10:00 (600 s). Respawns on 5-minute intervals (300 s): 600, 900, 1200, 1500, ...
- Descends from sky for 12.5 s before it can be picked up (lands ~612.5 s for the first).
- Lane: first urn always Yellow (York); alternates Yellow / Green each spawn. Spawns next to the bridge.
- Drop-off is on the opposite side of the map from the spawn.
- Pickup: hit with a melee attack (light or heavy).
- If not picked up within 3 minutes (180 s), it walks by itself to the drop-off via the Mid-Boss stairs, about halfway, before it respawns on the other side. It can still be picked up while walking.
- If picked up before next respawn time, the next respawn is delayed until the urn is delivered or expires. (Cursed Apple page: if carried on the next 5-min mark, it skips a turn and waits for the next mark.)

## Carrying
- Courier: +2 m/s move speed (cap 15 m/s), +1 stamina, +10% dash distance, +25% stamina regen, 100% slow resistance.
- Trailing team courier: extra Bullet/Spirit/Debuff resist and +5 m/s sprint speed; scales with souls deficit, max at 15% net worth difference. Resist max = 10% at minute 0, +1%/min, up to 40% at 30 min.
- Cannot use Ziplines/Teleporters, cannot parry.
- Drop on: enemy melee, melee-damage abilities, any stun; certain hero abilities (Mirage Traveler, Rem Tag Along, Sinclair Spectral Assistant teleport, Viscous Goo Ball). The Doorman Doors drop it at the door.
- Urn speaks every 6 s.
- Anti-stall: if not delivered within 45 s of pickup, value decays over a further 45 s to zero and it expires (45 + 45 = 90 s total).

## Reward
- Base soul bounty 250 + 70 per minute of game time, locked at the minute the urn spawns. Example (computed): spawn at 10 min -> 250 + 700 = 950; at 15 min -> 1300; at 20 min -> 1650.
- Released as Soul Orbs that can be denied; if not denied all go to courier; allied orb securers share.
- Courier also gets 4 random permanent buffs (tier by game time: LVL1 0 min, LVL2 10 min, LVL3 30 min). Tier values listed on the page (e.g. Weapon Damage +3% / +4% / +6%; Max Health +15 / +20 / +30; Spirit Power +2 / +3 / +4).
- Comeback bonus: trailing-team courier gets an extra soul bonus scaling with game time and deficit (exact formula UNVERIFIED - page says "see" but gives no numbers).
- Urn value does not increase by holding it.

## Update history (from page)
- 2026-07-09: Urn Runner sprint bonus +2m -> 0 (trailing +7m -> +5m); move speed bonus +3.5m -> +2m; stamina recovery +15% -> +25%; talk frequency 8 s -> 6 s.
- 2026-06-30: Unstable Rift separated into own objective (urn no longer triggers a King-of-the-Hill).

## UNVERIFIED
- Comeback soul bonus exact numbers.
- Whether the "walks by itself at 3:00" means 180 s after landing or after spawn start (page: "If the urn hasn't been picked up in 3 minutes").

## Machine-friendly table
| name | first_spawn_secs | interval_secs | notes |
|---|---|---|---|
| soul_urn | 600 | 300 | descends 12.5 s; first lane Yellow, alternates Yellow/Green |
| soul_urn_land | 612.5 | 300 | earliest pickup |
| soul_urn_auto_walk | n/a | n/a | starts walking 180 s after unclaimed |
| soul_urn_decay | n/a | n/a | decay starts 45 s after pickup, expires 45 s later |
| soul_urn_bounty | 600 | n/a | 250 + 70/min, locked at spawn |
