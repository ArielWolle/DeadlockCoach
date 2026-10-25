# Buffs and Pickups

Retrieved: 2026-10-25

Sources:
- https://deadlock.wiki/Powerup
- https://deadlock.wiki/Buff_Container
- https://deadlock.wiki/Crate
- https://deadlock.wiki/Healing_Snack
- https://deadlock.wiki/Sinner's_Sacrifice
- https://deadlock.wiki/Cursed_Apple (Map Timings, marked outdated by wiki)
- https://deadlock.wiki/Steam_Vent
- https://api.deadlock-api.com/v1/assets/map (entity counts and relative positions)
- Community: https://deadlockhandbook.com/souls/jungle-farming/

## Powerups ("Bridge Buffs" / Runes) - WIKI FACT
- Spawn as a glowing stone on two bridges near the Urn spawn/dropoff points, first at 5:00 then every 5:00. Claim with a Heavy Melee. Both sites spawn simultaneously; minimap icon shows the type. Uncollected ones are replaced at next interval.
- Duration 160 s; lost on death (also removable by Cursed Relic). Scale from 5 min to 40 min.
- 4 types (start -> end values):
  - Casting: +15 -> +65 Spirit Power, +12% -> +20% CDR
  - Gun: +12% -> +35% fire rate, +35% -> +70% ammo (reloads ammo on pickup)
  - Movement: +2 -> +4 stamina, +1.5 -> +4 m/s sprint, +20% -> +50% stamina regen, +40% -> +80% zipline speed
  - Survival: +200 -> +750 bonus health, +4 -> +40 fixed regen
- 5:00 types randomized; at 10:00 the other 2 types are guaranteed (wiki unsure whether a fixed order exists - UNVERIFIED).
- Positions (API `bridge_buffs`, relative left/top, world coords with map radius 10752): (0.317, 0.494) and (0.683, 0.506) - both near vertical centre, left and right of centre, i.e. on the side-lane/centre bridges. Not tied to a colour name by any source.

## Crates - WIKI FACT
- Wooden crate: 60% chance to drop Haunted Souls, value 23 + 2/min (scaling starts at match start, so value at 30 min is 83 per Handbook - consistent with 23+2*30).
- Tough Crate (teal, metal edges, added 2026-09-29): heavy melee only, 100% drop, 46 + 4/min.
- Break by shooting, melee, abilities, or dashing/sliding/walking through with momentum.
- Spawn 3:00, respawn 3:00 after break (delayed if loot left on ground uncollected). Exceptions: room under Times Square above Midboss pit spawns at 10:00 (respawn 3:00); Bell Tower top and Sunken Plaza spawn at 5:00 (respawn 3:00); Tunnels spawn 5:00 and respawn 5:00.
- Wiki: 518 crates. API: 423 crates + 74 tough crates (= 497) - UNVERIFIED reason for difference.
- Breakable souls are Haunted Souls (spent after Secured Souls; dropped at death location).

## Buff Containers (formerly Golden Statues) - WIKI FACT
- Same spawn/respawn timers as crates (3:00 / 3:00; Midboss-pit room 10:00; Tunnels 5:00 / 5:00).
- 50% chance to drop one permanent stat booklet. API: 166 `golden_statues`.
- Permanent buff table (shared with Sinner's Sacrifice):

| Stat | L1 (0 min) | L2 (10 min) | L3 (30 min) | Drop weight |
|---|---|---|---|---|
| Fire Rate | +1.5% | +2% | +2.5% | 1/10 |
| Max Ammo | +3% | +5% | +7% | 1/10 |
| Cooldown Reduction | +0.5% | +0.75% | +1% | 1/10 |
| Weapon Damage | +3% | +4% | +6% | 1/10 |
| Max Health | +15 | +20 | +30 | 2/10 |
| Spirit Power | +2 | +3 | +4 | 1/10 |
| Bullet Resist | +0.5% | +0.75% | +1% | 1/10 |
| Spirit Resist | +0.5% | +0.75% | +1% | 1/10 |
| Move Speed | +0.1 | +0.2 | +0.3 m/s | 0.5/10 |
| Ability Range | +0.25% | +0.5% | +0.75% | 0.5/10 |

Pseudo-random distribution (repeats less likely).

## Sinner's Sacrifice
See camp-types.md for HP, souls and jackpot rules. First spawn 8:00, respawn 5:00. Heavy melee jackpot = 4 buffs. API: 11 `vault` camps; 3 `bells` (Bell Tower machines). Bell Tower: hitting machines or any breakable at top rings the bell for the whole map (wiki: bell can ring once per 30 s; Bell Tower accessed only by a central rope).

## Healing Snacks - WIKI FACT
- Green glass apple with wings; walk near to pick up. Heals 10% max health over 4 s (20% in Street Brawl).
- Spawn 2:30, respawn 3:00 after pickup. Wiki: 36 snacks; API: 36 `healing_snacks`. Mostly high in the air (need movement), mostly along Greenwich and York (wiki). NOT shown on the minimap.
- API positions (relative left/top) span the whole map; heights z 640-1504, many 1000+. Example: (0.101,0.646), (0.127,0.381), (0.583,0.554), (0.895,0.359). Full list is in the API `entities.healing_snacks`.

## Steam Vents - WIKI FACT (stub page)
- Stand in it: hidden from enemy heroes, untargetable by target/LOS abilities, regen = 3 + 0.0025 x Max HP; visible to allies.
- Revealed when you damage/apply an effect to an enemy hero or an enemy enters the vent. Taking damage does not reveal.
- Found near Secret Shops, Haunt camps, Midboss. API: 47 steam_vents (one `obscured` in Sunken Plaza area).

## Other jungle-adjacent pickups
- Cockroaches in tunnels: 1 soul each (COMMUNITY: Deadlock Handbook; not on wiki pages fetched).
- Soul Urn: spawns 10:00 (+12.5 s descent), every 5:00; carrier cannot zipline/teleport; delivers to opposite lane side; rewards haunted souls and 4 permanent buffs (wiki Cursed_Apple). API soul_urn_spawns: 6 positions, pads: (0.194,0.500) and (0.793,0.505).
- Unstable Rift: spawns ~11:00 +/-1 min, every 7 +/-1 min after event ends (Cursed_Apple); Strategy page says every 8 min - conflict, UNVERIFIED. API rifts at (0.144,0.5) and (0.856,0.5).
- Midboss: 7/6/5 min respawn pattern; spawns Rejuvenator.
- Map-wide "Permanent Buffs" tiers at 0, 10, 30 min.

