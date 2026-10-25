# Jungle Routes and Clear-Order Heuristics

Retrieved: 2026-10-25

Labels used in this file:
- [WIKI] = stated on deadlock.wiki or in the deadlock-api.com data.
- [COMMUNITY] = opinion/heuristic from a third-party guide.
- [DERIVED] = my arithmetic or logic from [WIKI] facts.
- UNVERIFIED = no fetched source supports it.

Sources:
- https://deadlock.wiki/Haunt
- https://deadlock.wiki/Strategy (Jungling, Box Running sections)
- https://deadlock.wiki/Cursed_Apple (timings; flagged outdated by the wiki)
- https://deadlock.wiki/Sinner's_Sacrifice
- https://api.deadlock-api.com/v1/assets/map
- https://deadlockhandbook.com/souls/farming-routes/ (updated 2026-10-06, checked on the 2026-10-05 patch)
- https://deadlockhandbook.com/souls/jungle-farming/
- https://mobalytics.gg/deadlock/guides/jungle-guide (updated 2026-08-28, pre-"City Never Sleeps" naming; some numbers differ - see below)
- Not fetched / not used: ultimatedeadlock.com/2024/09/26/best-jungling-route (2024 video route, map since reworked 2025-02-25 and 2026-09-29).

IMPORTANT GAP: no fetched source gives a named, step-by-step camp-by-camp route on the current (post-rework) map. The routes below are timing-based frameworks. Do not present specific camp-id orders as "the" route.

## Timeline of what exists [WIKI]
| Time | Event |
|---|---|
| 0:00 | Troopers; Midboss present |
| 2:00 | Small camps (4) |
| 2:30 | Healing Snacks |
| 3:00 | Most crates/Buff Containers; Soul Wells in base |
| 5:00 | Medium camps; Powerups (every 5:00); crates/statues in Sunken Plaza and Bell Tower |
| 6:00 | Zipline Boost unlocks |
| 8:00 | Large camps; Sinner's Sacrifice camps |
| 10:00 | Soul Urn (then every 5:00); Midboss-pit room breakables; Tier 2 permanent buff values |
| ~11:00 (+/-1) | Unstable Rift |
| 12:00 | Guardians lose remaining resistances |
| 30:00 | Tier 3 permanent buff values |
Trooper waves every 30 s [WIKI Strategy]; 25 s after 20:00, 20 s after 35:00.

Respawn: Small 1:25, Medium 4:50, Large 5:35, Sinner's 5:00 [WIKI]. Timers start after full clear only.

## Core rule set
1. [WIKI] Clearing a Trooper wave is generally faster and pays more than a camp, so camp between waves (Strategy page). Wave interval 30 s.
2. [WIKI] Respawn starts only when all are dead -> clear fully near your side; leave one Haunt alive in camps on the enemy side to stall their respawn (Strategy page; Mobalytics says the same).
3. [WIKI] Roaming assassins can steal unsecured souls while you jungle. Haunted souls from camps are unsecured and can drop on death (Handbook: "haunted bank").
4. [WIKI] Heavy melee opens Tough Crates and is best on Sinner's Sacrifice jackpot (4 buffs).

## Early game (0:00 - ~5:00)
- [WIKI] 0:00-2:00 nothing to farm but lane.
- [WIKI] Small camps are at the guardian of the left side lane (each team) and both Broadway Guardians (Haunt page). "Left lane" is relative to a team's view; per Cursed_Apple only one side of each side lane has an easily accessible Small camp, which shifts lane balance.
- [COMMUNITY, Handbook] Push/clear wave, then take the nearest small camp while the next wave walks; be back for next wave.
- [COMMUNITY, Mobalytics] At 2:00 prefer pushing the lane / boxes; start small camps around 3:00. Crates appear 3:00.
- [WIKI] Healing Snacks from 2:30 (high up, need movement).
- Lane-to-jungle path: [DERIVED] from the API positions the small camps `dock_authority_camp` (0.167,0.560), `nw_utility_south_neutrals` (0.440,0.457), `se_utility_neutrals` (0.565,0.546), `park_club_camp` (0.793,0.455) are the four weak camps. Two are near the sides (x~0.17 / ~0.79), two near centre (x~0.44 / ~0.57) - consistent with wiki's "side lane guardian + Broadway guardians" description. Identify them on the minimap by the plain triangle with no lines.

## Mid game (~5:00 - ~12:00)
- [WIKI] Mediums at 5:00. Easy mediums (1-2 Tier II + weak Tier I) near Broadway small camps and near the right lane Guardian of both teams. District mediums are harder.
- [COMMUNITY, Mobalytics] Take mediums from about 6:00; if three Tier II spawn, get help or skip.
- [WIKI] 8:00 Large + Sinner's Sacrifice. [COMMUNITY, Mobalytics] At 8:00 push wave then take a nearby Sinner's camp, heavy melee, to take less retaliation. [COMMUNITY] Large camps efficiently from about 10:00; after that skip Small camps.
- [COMMUNITY, Handbook] From 8:00 the jungle gets ambush-prone; from 10:00 plan farming around Urn/Rift/Midboss and farm "toward objectives": camps on the path to the next fight.
- [WIKI] Powerups every 5:00 (160 s), Urn 10:00, Rift ~11:00: these pull teams into the same corridors the jungle uses.
- [WIKI] Sinner's Sacrifice: come with health (80 dmg per hit back, 320-640 total). Bell Tower triple rings a global bell.

## Late game (~12:00+)
- [WIKI] Large camps pay 181+1.95/min (e.g. computed 220 at 20 min). Tier III Haunts best used with Cultist Sacrifice (270% value + persistent buff) - common comeback/support tactic [WIKI Haunt page].
- [COMMUNITY, Handbook] "Don't farm when your team is grouping for Midboss/Rift/Urn/push"; losing a 5v6 costs more than a camp.
- [COMMUNITY] Heroes with fast camp-clear should take more jungle so lane-dependent heroes keep lane farm.

## Lane-to-jungle connections [WIKI + API]
- Districts are connected to lanes by buildings, alleys, tunnels (Cursed_Apple). Ropes (17) give vertical access; jump pads (17) launch horizontally; teleporters (2 pairs, from 0:00) cross the map; steam vents give hiding spots. Positions in map-layout.md.
- Chinatown/Bell Tower side: between Blue and Green on the Hidden King side; Haunted Lot toward Yellow on the Hidden King side; Plaza between Blue and Yellow on the Archmother side; Theater on the Archmother side near Green (inferred from district descriptions; wiki says Greenwich's immediate surroundings are Theater and Chinatown, York's are Haunted Lot and Plaza).
- Per API coordinates (team0 bottom), each side's quadrants hold roughly: lower-left Haunted Lot "slums_*" camps, lower-right Chinatown/"chinatown_*" + "park_*", upper-left "plaza_*", upper-right "theater_*". Ids and coordinates in map-layout.md.

## Clear-order heuristics (checklist for the coach)
1. [COMMUNITY] Never let a wave die without you; if wave is gone, souls are lost.
2. [COMMUNITY] Push first, then camp, return as waves clash.
3. [WIKI] Camp on timers: small 1:25 / medium 4:50 / large 5:35 - note the time you cleared fully to predict respawn.
4. [DERIVED] A camp left alive on enemy side never respawns; a camp you clear fully near your side respawns on schedule. Enemy opponent can do the same to you.
5. [WIKI] Break crates and Buff Containers on the way (3:00 respawn); they scale with time (wooden crate 23+2/min; at 30 min = 83 [COMMUNITY Handbook]).
6. [COMMUNITY] Shop when passing with spare souls; haunted souls are unsecured.
7. [COMMUNITY, Handbook] Leave farming for objectives: Midboss, Rift, Urn, Powerups.

## Discrepancies between sources (to avoid hardcoding)
- Medium camp first-spawn: wiki 5:00 (changed from 6:00 on 2026-05-22). Mobalytics (Aug) still lists 5:00 as spawn but advises ~6:00 for first clear.
- Large soul value: wiki 181; Mobalytics 196 and medium 71 (older numbers). Use wiki/API (181 and 68).
- Sinner's jackpot starts at 300 damage (wiki) / 400 (Mobalytics) - wiki says both: 3-or-fewer bulbs picks speed, 1-or-0 bulbs starts Jackpot (400). UNVERIFIED which minigame description is current.
- Rift respawn: 7 +/-1 min (Cursed_Apple) vs 8 min (Strategy).
