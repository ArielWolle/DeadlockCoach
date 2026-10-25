# Map Layout (Cursed Apple / dl_midtown)

Retrieved: 2026-10-25

Sources:
- https://deadlock.wiki/Cursed_Apple (alias of Map; page flagged "outdated" in its timings section)
- https://deadlock.wiki/Zipline
- https://deadlock.wiki/Teleporter
- https://deadlock.wiki/Jump_Pad
- https://deadlock.wiki/Steam_Vent
- https://deadlock.wiki/Minimap
- https://api.deadlock-api.com/v1/assets/map (radius, objective_positions, zipline_paths, neutral_camps, entities)

## Lanes (WIKI FACT)
Three color-coded lanes; each has its own zipline in the same color.

| Lane | Color | Character | Locations (enemy-ward order not given by wiki) |
|---|---|---|---|
| York | Yellow | Industrial: trucks, docks, loading bays | Stock Exchange, Shipyard Docks, Fairfax Factories |
| Broadway | Blue | Middle lane, bright neon/billboards, shortest route; Midboss pit below Times Square | Uptown, Times Square, Downtown |
| Greenwich | Green (also called Park) | Overgrown, vines and trees | Umbria University, Central Park, The Brownstones |

Map has 3 lanes since the 2025-02-25 rework (old 4th lane Purple/Park removed).

Minimap orientation (WIKI FACT): minimap is always flipped so YOUR base is at the bottom. Hidden King team sees Yellow - Blue - Green left to right; Archmother team sees Green - Blue - Yellow. Spectator default has Archmother side as North.

Teams: The Hidden King = Orange, base is a run-down factory with speakers. The Archmother = Blue, base is a neoclassic museum.

## Districts = "the jungle" (WIKI FACT)
Four districts fill the gaps between lanes; no Guardians and no Troopers there, only Haunts, breakables, machines.

| District | Side | Notes |
|---|---|---|
| Theater District | Archmother side | Inside a large theater; Stage Hands only here; glass roof dome entry; Tier 2 Underhand camp in tunnel below, reached from the central canal. |
| Chinatown District | Hidden King's right-hand side | Lanterns; Festival Spirits only here; Bell Tower with 3 Sinner's Sacrifices at the top ("triples"), entered only by a central rope, rings map-wide when anything at top is damaged; Crabbage Pot camp in florist/nursery bordering Greenwich and canal; Tier 3 Gutter Ghoul camp on street near Broadway exit. |
| Haunted Lot District | Hidden King's left-hand side | Green lighting, fungus; garage toward Blue with "doubles" (2 machines + Gutter Ghoul II); basketball court Underhand camp (ball is a physics object); Slum Shroom building near York; Barrel Mimic T2 camp between garage and court. |
| Plaza District | Archmother side | Fairfax statue; underground Sunken Plaza with purple fog: no hearing, no vision in/out, stamina drains and does not regen. Gutter Ghouls inside and out; Past Dues camp below statue. Contains the Bull statue (trivia). |

Side-lane association (inferred, see below): Haunted Lot sits toward Yellow on Hidden King side; Chinatown between Blue and Green on Hidden King side; Plaza between Blue and Yellow on Archmother side (Sinner's Sacrifice wiki page: "Sunken Plaza ... Between the Blue and Yellow lanes on the Archmother's side"; Bell Tower "Between the Blue and Green lanes on the Hidden King's side").

## Coordinate system (API FACT)
`radius` 10752. Relative coords: `left_relative = (x + 10752) / 21504`, `top_relative = (10752 - y) / 21504`. Team0 core at (0.2905, 0.92) i.e. bottom; team1 core at (0.4095, 0.0) i.e. top. The map is rotated: the cores are offset horizontally (team0 lower-left-of-centre, team1 upper-right-of-centre) and positions are 180-degree point symmetric about (0.5, 0.5) for many entities (e.g. rifts 0.144 / 0.856; teleporters mirrored).

Inference (UNVERIFIED by any source that says so): API team0 = bottom base = the viewing team's base when drawing in minimap orientation; from district positions in the API camp list (Chinatown/bell at lower-right, Haunted Lot "slums" at lower-left, Theater at upper-right, Plaza "plaza_*" upper-left) team0 is consistent with The Hidden King. Treat as likely, not confirmed.

## Objective positions in relative terms (API FACT)
Format (left, top). Bottom = team0.

| Objective | team0 (bottom) | team1 (top) |
|---|---|---|
| Core / Patron area | (0.291, 0.920) | (0.410, 0.000) |
| Titan (Patron) | (0.391, 0.824) | (0.510, 0.076) |
| Tier1 lane objective (Guardian) slot 1 / 3 / 4 | (0.069,0.536) / (0.450,0.533) / (0.777,0.542) | (0.123,0.358) / (0.450,0.367) / (0.828,0.364) |
| Tier2 (Walker) slot 1 / 3 / 4 | (0.158,0.670) / (0.390,0.612) / (0.712,0.685) | (0.198,0.216) / (0.510,0.289) / (0.750,0.228) |

Three lane slots are labelled 1, 3, 4 in the API (slot 2 absent: 4-lane remnant). Slot 3 is the centre lane (x about 0.45-0.51), slots 1 and 4 are the left and right side lanes. Which of 1/4 is Yellow vs Green is not stated by the API.

Midboss: center, in sewer under Broadway (wiki); mid-room "mid boxes" above it. API has no midboss entry in `entities`; cosmic veils near (0.45-0.55, 0.48-0.52).
Unstable Rift sites: (0.144, 0.500) and (0.856, 0.500) (side lanes, wiki: Greenwich or Yellow).
Soul Urn pads: (0.194, 0.500) and (0.793, 0.505).
Powerup/bridge buff sites: (0.317, 0.494) and (0.683, 0.506).
Shops (API): 9 listed (kind lane/secret/base). Secret shops: (0.318, 0.448) and (0.682, 0.552). Wiki: 10 shops incl. 2 secret, 1 per base, 1 beside each Guardian.

## Mobility

### Ziplines (WIKI FACT + API)
- Run from each base along each lane through all its locations to the enemy base; usable only as far as your Troopers have pushed.
- Speed fixed at 18 m/s inner lane, 21 m/s outer lanes. Refreshes air dash and double jump. Down key drops you keeping momentum 2 s.
- Zipline Boost (default F): +80% for 32 s, cooldown 360 s, first available 6:00. Destroying a Base Guardian permanently makes that line an 80% "Express Line". Movement Powerup adds +40-80%.
- Taking damage from enemy hero/objective locks zipline attach for 3 s. Neutrals (Haunts, Sinner's Sacrifice) and enemy Troopers do NOT lock it.
- Hit by an enemy hero while on the line (before 3 s of riding): knocked off, 15% max HP dmg, 2.5 s stun after landing. After 3 s riding you are immune to knock-off but take +53.6% extra Hyperline damage.
- API has 3 zipline_paths, colors Blue #29b1cc, Green #59b247, Yellow #F1CC30. Origin world coords: blue (-1280,-10774), green (-1088,-10790), yellow (1218,10784). The yellow origin is at the TOP base (y positive), blue/green at the BOTTOM base; the paths are one-way polylines with lateral drift of about 2560 world units. Use API images (below) rather than reconstructing.

### Jump pads / air vents (WIKI FACT + API)
- Launch in direction of airflow; landing spot marked by white graffiti; you can air-strafe. Damage within 0.9 s of launch: stun until 0.2 s after landing, 30% slow 2 s, 15% max HP damage.
- API lists 17 `bounce_pads` with start and `target` world coords. Relative start positions (left, top): (0.110,0.324), (0.216,0.533), (0.301,0.381), (0.357,0.829), (0.390,0.475), (0.418,0.407), (0.419,0.354), (0.468,0.163), (0.525,0.829), (0.585,0.595), (0.611,0.527), (0.615,0.681), (0.651,0.163), (0.694,0.432), (0.707,0.610), (0.784,0.473), (0.851,0.677). Targets use the same formula; two pads (0.216,0.533) and (0.390,0.475) and (0.784,0.473)+(0.611,0.527) all feed the bridge-buff sites at (0.317,0.494) / (0.683,0.506).

### Teleporters (WIKI FACT + API)
- 2 pairs / 4 teleporters, near underground Secret Shops, available from game start. 4 s channel, small speed boost on arrival (wiki Oct 2024: +3 m/s for 5 s). Urn carrier cannot use them.
- API positions (relative): (0.319,0.417), (0.333,0.630), (0.667,0.371), (0.681,0.583). API gives each `target` in world coords: (-3904,1792) -> (3584,2656); (-3584,-2784) -> (3904,-1664); (3584,2784) -> (-3904,1664); (3904,-1792) -> (-3584,-2656). Each pair connects one side to the mirrored opposite side of the map (horizontal traversal).

### Ropes, vents, other
- 17 climb ropes (API), vertical travel; Bell Tower accessible only by rope.
- Steam Vents: 47 (API), hide you. Cosmic Veils / Juke Rooms: 30 veils (API); dead-end rooms with a shadow figure in the doorway.
- Tunnel Vents: only certain heroes (Rat King, Rem, Calico ability) use them to move through tunnels. Underground "stairs" icon shows only underground.

## Minimap data the coach can use
Map images from API `images`: minimap, plain, frame, mid, mid_tunnels, rat_tunnels at https://assets-bucket.deadlock-api.com/assets-api-res/images/maps/ (minimap_midtown_mid.png, minimap_midtown_mid_tunnels.png, minimap_midtown_rat_tunnels.png, minimap_frame.png).

## Neutral camp positions (API `neutral_camps`, relative left/top, team0 at bottom)
API kinds: weak = small, medium = medium, strong = large, vault = Sinner's Sacrifice camp. Names are internal ids; family is not given by the API (family claims come from wiki, see camp-types.md). Zone labels are my reading of the name only (UNVERIFIED).



### weak (4)
| id | left | top |
|---|---|---|
| dock_authority_camp | 0.167 | 0.560 |
| nw_utility_south_neutrals | 0.440 | 0.457 |
| park_club_camp | 0.793 | 0.455 |
| se_utility_neutrals | 0.565 | 0.546 |

### medium (25)
| id | left | top |
|---|---|---|
| chinatown_fish_neutrals | 0.589 | 0.662 |
| construction_camp | 0.130 | 0.405 |
| dept_store_camp | 0.411 | 0.540 |
| hq_01_camp | 0.292 | 0.381 |
| hq_roof_camp | 0.235 | 0.423 |
| ne_theater_upstairs_camp | 0.771 | 0.315 |
| nursery_downstairs_camp | 0.708 | 0.589 |
| nw_bank_camp | 0.099 | 0.302 |
| oracle_camp | 0.591 | 0.461 |
| park_chapel_camp | 0.869 | 0.606 |
| park_library_roof_camp | 0.765 | 0.580 |
| park_mansion_roof_camp | 0.793 | 0.397 |
| plaza_rink_02_camp | 0.387 | 0.381 |
| slums_alley_camp | 0.314 | 0.579 |
| slums_bodega_down_camp | 0.266 | 0.532 |
| slums_garage_down_camp | 0.346 | 0.559 |
| slums_roof_camp | 0.229 | 0.612 |
| slums_storage_camp | 0.292 | 0.679 |
| sw_asylum_camp | 0.872 | 0.720 |
| sw_bodega_upstairs_camp | 0.244 | 0.671 |
| theater_down_camp | 0.699 | 0.452 |
| theater_lobby_camp | 0.677 | 0.328 |
| theater_stage_02_camp | 0.676 | 0.414 |
| theater_upstairs_01_camp | 0.655 | 0.327 |
| theater_upstairs_02_camp | 0.720 | 0.327 |

### strong (11)
| id | left | top |
|---|---|---|
| basketball_camp | 0.271 | 0.583 |
| chinatown_alley_camp | 0.611 | 0.630 |
| chinatown_bell_02_camp | 0.646 | 0.681 |
| chinatown_street_camp | 0.680 | 0.686 |
| dock_camp | 0.070 | 0.500 |
| park_gate_camp | 0.927 | 0.500 |
| plaza_corner_camp | 0.323 | 0.313 |
| plaza_rink_01_camp | 0.362 | 0.381 |
| plaza_rink_03_camp | 0.461 | 0.318 |
| slums_warehouse_camp | 0.336 | 0.704 |
| theater_stage_01_camp | 0.711 | 0.414 |

### vault (11)
| id | left | top |
|---|---|---|
| chinatown_bell_1 | 0.651 | 0.588 |
| dock_authority_vault | 0.225 | 0.555 |
| drug_store_camp | 0.603 | 0.545 |
| hotel_01_camp | 0.397 | 0.452 |
| park_club_vault | 0.775 | 0.437 |
| park_library_vault | 0.769 | 0.558 |
| plaza_pit_vault1 | 0.399 | 0.359 |
| plaza_pit_vault2 | 0.399 | 0.403 |
| slums_garage_up_camp | 0.348 | 0.574 |
| theater_balcony_camp | 0.693 | 0.371 |
| york_radio_vault | 0.234 | 0.443 |
