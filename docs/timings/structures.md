# Structures: Lane Guardians, Walkers, Base Guardians, Shrines, Patron

Sources (all retrieved 2026-10-25):
- https://deadlock.wiki/Lane_Guardian (page last modified 2026-10-04; latest update entry 2026-09-16)
- https://deadlock.wiki/Walker (last modified 2026-10-13; latest update entry 2026-09-29)
- https://deadlock.wiki/Base_Guardian (last modified 2026-10-04; latest update entry 2026-05-22)
- https://deadlock.wiki/Shrine (last modified 2026-10-20; latest update entry 2026-04-30)
- https://deadlock.wiki/Patron (last modified 2026-10-25; latest update entry 2026-01-22)
- https://deadlock.wiki/Souls (last modified 2026-10-25) for bounty table and splits
- https://deadlock.wiki/Extra_Slots (last modified 2026-10-04)
- https://deadlock.wiki/Zipline (last modified 2026-10-04)

Structures do not "spawn" on a clock; they exist from 0:00 and have a destroy order. Unlock = what must die first. Resistances decay with the in-game clock.

## Order of destruction
1. Lane Guardian (3 per team, one per lane)
2. Walker (3 per team; can only be damaged after its lane Guardian is destroyed)
3. Base Guardians (3 sets of 2 per team, one set per lane entrance)
4. Shrines (2 per team): invulnerable until at least one set (pair) of Base Guardians is destroyed; also immune to damage from outside their platform
5. Patron: invulnerable until both Shrines destroyed (phase 1 12000 HP), then Weakened Patron phase 2

## Lane Guardian
- HP 5500. DPS 116, range 34 m. Souls 1375 (page; bounty +10% on 2026-09-16, 1250 before that after 2026-06-30 cut from 1500).
- Invulnerable when no enemy trooper or player within 24 m. Targets players that damage allies within 15 m of it. Melee at 5 m; parry stuns it 2 s then no melee for 5 s.
- Damage resist (all except Pure) decays linearly 50% at 0:00 to -50% at 12:00 (720 s); table: 0:00 50%, 3:00 25%, 6:00 0%, 9:00 -25%, 12:00+ -50%. Also 10% Bullet Resist and -25% (take more) vs players. Bullet formula: Damage = BD * [0.5 + t/720] * 1.25 * 0.9 (t in secs, up to 720).
- No backdoor protection. Headshottable (brazier).
- Reward: split to alive heroes of killing team per Souls page (see Bounty splits).

## Walker
- HP 6000 with 3 Walkers alive; 9000 when 2 remain; 12000 when 1 remains (flat +3000 and heal each time another Walker dies).
- DPS 125, range 24 m. Invulnerable when no enemies within 22 m. Gives nearby allied heroes (28 m) 15% Bullet and Spirit resist.
- Damage resist decays linearly from 65% at 0:00 to -65% at 18:00 (1080 s). Table: 0:00 65%, 3:00 43.33%, 6:00 21.67%, 9:00 0%, 12:00 -21.67%, 15:00 -43.33%, 18:00+ -65%. Plus 35% Bullet Resist; extra weapon-damage reduction with 3+ enemies within 51 m (20% at 3 up to 50% at 6).
- Attacks (Spirit damage): Fire Beam 2% max HP/s; Stomp 350 + 15% max HP, 2 s stun, 15 m radius, 7 s cooldown; Fireballs 6 x 200 + fire 25 dps for 5 s, -10% bullet & spirit resist, 10 s cooldown.
- Backdoor protection if lane not pushed to it: 65% damage reduction from players and 65 HP/s regen.
- Souls: 3675 total. Walker page text: 40% (1470) split among nearby attackers (damaged in last 20 s) and 60% (2205) split among all six. NOTE conflict: Souls page (update 2026-09-16) says objective share to nearby heroes reduced from 30% to 25%; Walker page text may be outdated. Treat 25% / 75% as newer.
- Each Walker destroyed unlocks 1 Extra Slot for the team (1st, 2nd, 3rd walker -> bottom-left, top-right, bottom-right). Base slots 9 + 3 extra.

## Base Guardian
- HP 4000 (reduced from 5500 on 2026-03-10). DPS 116, range 27.94 m; melee within 6.35 m (cannot be parried; no headshot damage).
- Resist: Enemy-scaling Bullet Resist starts at 40%, -20% per nearby enemy hero (0% at 2+ enemies); Bullet Resist 20% (increased from 10% on 2026-05-22); -25% player vulnerability.
- Backdoor protection: 65% damage resist and 65 HP/s regen.
- Souls: 1000 each (Souls page).
- On destroy: permanent 80% speed boost for attacking team's zipline in that lane (Express Line). Zipline page: base zipline speed 18 m/s inner lane, 21 m/s outer lanes.
- Destroying a pair makes Shrines damageable.

## Shrine
- HP 5000 for the first shrine; when one dies, the other becomes 10000 HP (and heals). Bullet Resist 20%; enemy-scaling Bullet Resist 60% -20% per nearby enemy hero (0% at 3+ heroes).
- NOTE: Shrine page infobox says Bullet Resist 20%; update 2026-04-30 text says "10%". Page body says 20% -> use 20%, UNVERIFIED which is current.
- Backdoor protection 65% damage resist, 65 HP/s regen. Patron defends shrines under attack with a beam/explosion (300 + 20% HP spirit damage after 3.5 s, curses).
- Souls: 2000 each (bounty added 2026-03-06).
- On destroy: allied troopers in that lane (Blue lane once either shrine is destroyed) become Super Troopers (+60% damage, +40% health; yield 15% less bounty). See troopers-and-waves.md. Wiki note says mid-lane upgrade rule may have been reverted undocumented.

## Patron
- Phase 1: 12000 HP, regen 120 HP/s (+75 HP/s Backdoor Protection when no enemy troopers near), laser 440 + 5% HP spirit DPS, range 76 m. Unkillable until both Shrines destroyed.
- Max HP scaling: Phase 1 +250 per minute starting at 20 min (1200 s); Phase 2 +450 per minute starting 1 min into phase (per page "Scaling Starts At: 1 minute").
- After first half of its health bar: becomes invulnerable briefly, then Weakened Patron (12000 base HP). Immune from outside the pit; 50% damage resistance when 1 defender in the pit for >2 s, 100% with 2+ defenders. Damage Pulse 75 dmg/s to up to 3 enemies within 19 m; Pit Explosion 300 + 20% HP after 3.5 s, 15 s cooldown.
- Killing phase 1 shortens remaining respawn time of dead defenders by up to 20 s (Last Stand buff). Patron "1st death time" constant 12 s (2025-09-04).
- Souls: 0.

## Bounty splits (Souls page, update 2026-09-16)
- Guardian: split 25% to nearby heroes and 75% among the whole team (including nearby). Page notes Guardian 1375, Walker 3675, Base Guardian 1000, Shrine 2000, Patron 0.
- Strategy page (older, oldid 157579, modified 2026-09-14) lists 1250 / 3500 / 1000 / 2000 and 30%/70% splits: OUTDATED values, shown only for reference.

## Hero damage reductions to objectives
- Abilities scaled to 50% vs objectives: Bebop Hyper Beam, Infernus Flame Dash, McGinnis Heavy Barrage, Mina Nox Nostra, Paradox Pulse Grenade, Pocket Affliction, Seven Storm Cloud, Sinclair Vexing Bolt, Solomon Setup Bishop, Victor Aura of Suffering, Warden Last Stand. Haze Fixation 20%.

## UNVERIFIED
- Any clock time at which structures become damageable by design (none stated; only state-based).
- How many seconds a Walker/Guardian takes to kill at a given clock (not on pages; would need hero DPS).
- Base Guardian per-set vs per-guardian bounty (1000 each stated; page row "Base Guardian 1000").

## Machine-friendly table
| name | first_spawn_secs | interval_secs | notes |
|---|---|---|---|
| lane_guardian | 0 | n/a | HP 5500; bounty 1375; resist 50% -> -50% by 720 s |
| walker | 0 | n/a | HP 6000/9000/12000; bounty 3675; resist 65% -> -65% by 1080 s; needs guardian dead |
| base_guardian | 0 | n/a | HP 4000 x2 per lane; bounty 1000 each; zipline +80% when set dies |
| shrine | 0 | n/a | HP 5000 then 10000; bounty 2000; needs 1 base-guardian pair dead |
| patron_phase1 | 0 | n/a | HP 12000 (+250/min after 1200 s); needs both shrines dead |
| patron_phase2 | n/a | n/a | HP 12000 (+450/min); 50%/100% dmg resist with 1/2+ defenders in pit |
| guardian_resist_zero | 360 | n/a | resist 0% at 6:00; 720 s = -50% (min) |
| walker_resist_zero | 540 | n/a | resist 0% at 9:00; 1080 s = -65% (min) |
