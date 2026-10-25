# Deadlock Jungle Reference (for DeadlockCoach)

Retrieved: 2026-10-25

Sources (primary):
- https://deadlock.wiki/Haunt (also /Neutral_Creeps, /Jungle), /Cursed_Apple (alias /Map), /Powerup, /Buff_Container, /Crate, /Healing_Snack, /Sinner's_Sacrifice, /Zipline, /Teleporter, /Jump_Pad, /Steam_Vent, /Minimap, /Strategy
- https://api.deadlock-api.com/v1/assets/npc-units
- https://api.deadlock-api.com/v1/assets/map
- Community: deadlockhandbook.com (farming-routes, jungle-farming), mobalytics.gg/deadlock/guides/jungle-guide

Game state: post "City Never Sleeps" rework (2026-09-29). Neutrals are now called Haunts. Wiki pages were edited as recently as 2026-10-25.

## Files
| File | Contents |
|---|---|
| camp-types.md | Small/Medium/Large/Sinner's camp stats, souls, respawns, families, who clears them |
| jungle-routes.md | Timeline, early/mid/late framework, lane-to-jungle links, heuristics with [WIKI]/[COMMUNITY]/[DERIVED] labels |
| map-layout.md | Lanes, colors, districts, objective positions (relative coords), ziplines, jump pads, teleporters, all 51 camp ids and coordinates |
| buffs-and-pickups.md | Powerups, crates, Buff Containers, snacks, steam vents, urn/rift |
| jungle-tactics.md | Invading, counter-jungling, minimap cues |

## Key numbers at a glance [WIKI/API]
Small 129 HP / 41 souls / spawn 2:00 / respawn 1:25. Medium 322 / 68 / 5:00 / 4:50. Large 1133 / 181 / 8:00 / 5:35. Sinner's Sacrifice 8:00 / 5:00. Crates/Buff Containers 3:00 / 3:00. Snacks 2:30 / 3:00. Powerups 5:00 every 5:00.

## Conventions
- Facts carry the source in the file. Community opinion is labelled [COMMUNITY]. Numbers I computed are [DERIVED]. Anything unsupported is UNVERIFIED.
- Relative coordinates: left = (x + 10752)/21504, top = (10752 - y)/21504; own-base (team0) at bottom.

## Known gaps
- No fetched source gives a named, current, camp-by-camp route.
- Camp counts disagree between wiki and API (medium 22 vs 25; large 11 vs 12 on one wiki table).
- Sinner's Sacrifice soul value: wiki 310 vs API 155; Rift respawn 7 vs 8 min; Midboss timer wording; wiki Map timings page is flagged outdated.
- Which API team (team0) equals Hidden King is inferred, not stated.
- API `npc-units` has no respawn/camp-composition data; camp composition (which Haunts per camp) not retrieved per camp.
- Healing snack/crate/statue coordinates exist in the API but are not listed individually here except examples.
