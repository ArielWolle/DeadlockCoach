# Camp Types (Haunts / Neutral Camps)

Retrieved: 2026-10-25

Sources:
- https://deadlock.wiki/Haunt (alias of Neutral_Creeps / Jungle; page last modified 25 Oct 2026)
- https://deadlock.wiki/Cursed_Apple (alias of Map; map icon table is marked "outdated" by the wiki)
- https://deadlock.wiki/Sinner's_Sacrifice
- https://api.deadlock-api.com/v1/assets/npc-units (class_name `neutral_*`)
- https://api.deadlock-api.com/v1/assets/map (`neutral_camps`)
- Community: https://deadlockhandbook.com/souls/jungle-farming/ , https://mobalytics.gg/deadlock/guides/jungle-guide

Terminology: the game now calls neutrals "Haunts" (City Never Sleeps rework, 2026-09-29). Older guides say "Neutrals/Denizens".

## Tier table (WIKI FACT, confirmed by API)

| Camp size (minimap) | Minimap icon | Health | Souls (base) | Soul gain /min | Bullet / Spirit resist | Initial spawn | Respawn (after FULL clear) |
|---|---|---|---|---|---|---|---|
| Small / Tier I | triangle, no lines | 129 | 41 | +0.44 | 50% / 45% | 2:00 | 1:25 |
| Medium / Tier II | triangle + 1 line | 322 | 68 | +0.73 | 50% / 45% | 5:00 | 4:50 |
| Large / Tier III | triangle + 2 thick lines | 1133 | 181 | +1.95 | 60% / 55% | 8:00 | 5:35 |
| Sinner's Sacrifice ("vault") camp | machine icon | machine 500 HP (+ Haunts) | see below | see below | n/a | 8:00 | 5:00 (solo) or when whole camp cleared |

API cross-check (`neutral_*_weak/normal/strong`): max_health 129 / 322 / 1133; gold_reward 41 / 68 / 181; `gold_reward_bonus_percent_per_minute` 1.08; strong has bullet resist 60 and ability (spirit) resist 55 via intrinsic modifiers.

Scaling (WIKI FACT): Haunt health +2.1%/min, damage +0.5%/min, soul value +1.08% of base per minute counted from match start (not spawn).
Reward per player: `floor(B * (1 + 0.0108 * M) / C)` (B base, M match minutes, C allied contributors who damaged within last 5 s). Souls are Haunted Souls; no Soul Orbs.

Computed from that formula (single killer, my arithmetic, not a wiki table):
| Match min | Small | Medium | Large |
|---|---|---|---|
| 10 | 45 | 75 | 200 |
| 20 | 49 | 82 | 220 |

## Damage (API)

Per shot `bullet_damage` at base (before +0.5%/min), cycle_time 0.5 (2 shots/s):
- Small 2.415 (about 4.8 dps)
- Medium 6.125 (about 12.25 dps)
- Large 9.1 (18.2 dps; API lists `damage_per_second` 18.2)
Walk/run speed on Large Gutter Ghoul: 80 (API). Sight range 1500 (API). Melee attempt range 150 (API).
Mobalytics (community, older) lists 2.5 / 6 / 9 base damage - consistent in magnitude.
Melee hits and family special attacks deal more; see families below.

## Camp composition rules (WIKI FACT)
- Small camps: only Tier I, always 3 Haunts.
- Medium camps: Tier I or II Haunts; 6 different composition combos exist. Some have just 1-2 Tier II (+ weak Tier I) and are approachable early; "district" ones are harder.
- Large camps: either 3x Tier III, 1x III + 2x II, or one of each tier.
- One Haunt family per camp. Camps are static (same composition every respawn).
- Haunts spawn dormant. Damage is ineffective unless attacker is within 38.1 m or the Haunt is active. One provoked Haunt aggros the whole camp onto that hero.
- Leash: Haunts chase only a limited distance from camp centre (example: sunken plaza camp 15.2 m / 600 su), +3 m if line of sight, then reset.
- Crit: shoot the eye for headshot-style crit.
- Respawn timer starts only when ALL Haunts (and all Sinner's Sacrifice machines in a hybrid camp) are dead.

## Camp counts

| Source | Small | Medium | Large | Sinner's camps |
|---|---|---|---|---|
| Wiki Cursed_Apple icon table (marked outdated) | 4 | 22 | 11 | 11 |
| Wiki Minimap table | 4 | 22 | 12 | 10 |
| API map `neutral_camps` (kinds weak/medium/strong/vault) | 4 | 25 | 11 | 11 |

UNVERIFIED which count is current; API medium (25) differs from wiki (22). Sinner's Sacrifice page: 15 machines at 11 locations (6 solo, 2 hybrid with 1 machine + 2 Medium Haunts, 2 hybrid with 2 machines + 1 Medium Haunt, 1 location with 3 machines). Camp ids and relative positions: see map-layout.md.

## Sinner's Sacrifice economics (WIKI FACT)
- 500 HP, melee only: light melee 50, heavy melee 100. Retaliates 80 dmg per melee hit (min 320 / max 640 total).
- Jackpot minigame begins once at 3 or fewer bulbs (300 dmg); 40% fast / 60% slow. Final jackpot hit: heavy melee = 4 random permanent buffs, light = 1.
- Souls: wiki says value starts 310 and +3.35/min (distributed: light = value/20, heavy = value/10, final = value/2 + one heavy payout). Notes: API `neutral_sinners_sacrifice` gold_reward is 155 (+1.08%/min) - conflict, probably the wiki's 310 is a 2x internal value after a bounty-reduction patch; UNVERIFIED which is current.
- Mobalytics (community, older) quotes 310 +3/min.

## Haunt families and how they behave (WIKI FACT)

| Family | Tiers | Where | Special attack / traits |
|---|---|---|---|
| Gutter Ghoul | I, II, III | everywhere (most common) | II/III lob toxic waste, puddle = Spirit DoT + slow; III throws 10, tracks. |
| Underhand | I, II, III | rooftops on inner side of side lanes, tunnels, Haunted Lot basketball court; NOT near Broadway | No melee. II/III laser, ground trail 30/50 Spirit per tick. |
| Barrel Mimic | II, III | Haunted Lot (II); shipyard docks near York middle (III) | 4 s channel then AoE Spirit explosion (orange telegraph II, red larger III). |
| Specimen | I, II | Times Square camps only (Broadway) | Tier II explodes on death: 75 Spirit + slow. Archmother-side T2 Times Square camp is Gutter Ghouls instead. |
| Past Due | I, II | Greenwich camps + one camp under the Plaza statue | I ranged only; II melee only. II book barrage, 20 Spirit/book, ends with Sleep. |
| Stage Hand | II, III | Theater District only | Same laser trail as Underhand. |
| Crabbage Pot | I, II, III | Chinatown (florist/nursery), one T3 camp at Central Park | II/III thorn circle: 125 / 187 BULLET damage (only family with no Spirit special). Melee can be parried. |
| Festival Spirit | II, III | Chinatown only | No melee. Homing fireworks 66 Spirit AoE; 3 rockets (II), 5 (III). |
| Slum Shroom | II, III | one ruined Haunted Lot building (2 camps, 6 units) | Spore mines: 50 Spirit + slow + -50% healing + 5/tick poison; 3 mines (II), 4 (III). |

Sources of family location claims: Haunt and Cursed_Apple wiki pages.

## Recommended hero type to clear

WIKI FACT (Haunt page):
- Small camps: any hero from minute 0 of availability (2:00). Lifesteal abilities (Mina Rake, Lady Geist Life Drain) turn them into healing.
- Medium: most heroes can do the easy ones early; district mediums are hard for "poor ability to clear camps".
- Large: high bullet/spirit resist makes them very hard; only high damage + resist shred (named: Haze, Wraith) find it worthwhile later.
- Items that help (Strategy page): Cultist Sacrifice (kill a Tier III Haunt for 270% of its value plus a long-lasting buff), Ricochet.
- Strategy page: no dedicated Jungler role; some heroes clear faster.

COMMUNITY OPINION (Deadlock Handbook): heroes with strong area damage, turrets or summons clear camps fastest and should take more jungle; carries farm most, supports take less but still some.
Mobalytics: Medium camps hard before ~6:00 if 3 mediums spawn; ask ally for help or skip.

GAP: no source gave a per-hero clear-speed ranking. UNVERIFIED which individual heroes are best beyond Haze/Wraith (wiki) and lifesteal examples.
