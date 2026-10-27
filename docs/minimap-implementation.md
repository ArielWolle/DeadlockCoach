# Minimap detection and coaching

## Implementation

- `crates/vision/src/minimap.rs`: calibrated 16:9 crop, lane polylines, HSV colour masks,
  connected components, circular hero-ring support, vase shape detection, short-history tracker.
- `crates/core/src/lib.rs`: `MinimapState` and per-lane observations in `GameSnapshot` JSON.
- `crates/engine/src/map_rules.rs`: wave defence, conditional jungle window, Urn and Midboss cues.
- Capture/transport: native-resolution PNG minimap accompanies the bandwidth-limited full JPEG.
  The Brain records it as `*.minimap.png`. Older Agents/recordings fall back to the JPEG crop.

This is calibration and rule-based computer vision, not a trained model. Geometry was calibrated
against the user's 275-frame recording `1792978351_frames` on the current map/HUD. Other aspect
ratios, HUD offsets and map patches need recalibration. Screenshot capture only.

## Evidence and uncertainty

| Feature | Evidence | Limits |
| --- | --- | --- |
| Lane pressure | Proportion of sampled lane-line pixels in lane colour versus red; requires three consistent frames | A pressure estimate, not exact wave position or a verified measure of lane ownership |
| Troopers | Small solid white/red components near a lane; known furniture and hero-ring areas excluded | Counts are marker detections, not exact unit counts; overlapping or occluded dots are missed |
| Hero visibility | Team-colour masks (cream-white allies, red-orange enemies), then portrait-sized circle search with greedy explain-away (see below) | Matches 12 of 12 hand-labelled allies and 10 of 10 enemies on three fixtures; precision on other HUD themes, colour-blind settings and heavy stream compression is unmeasured. Not visible does not mean absent or dead |
| Hero tracks | Per-team nearest-neighbour matching with a time-growing gate, velocity from consecutive sightings, last-seen position kept for 45 s | Identity is per track, not per hero name; two heroes crossing paths can swap tracks |
| Midboss | Gold winged icon near centre, supported in at least two of three frames | Positive availability evidence only. Absence never establishes a kill or starts a respawn timer |
| Urn marker | Cyan cap/body shape, furniture exclusions, consecutive spatial support | Cyan portraits, delivery outlines, and occlusion can still cause misses |
| Urn on ground | Stationary confirmed vase at a bridge spawn position | Do not infer ground state elsewhere |
| Your carry | Literal `DELIVER THE URN` HUD text plus confirmed vase | Disabled while spectating |
| Ally/enemy carry | Vase and same-team ring moving together over two frames | Heuristic. No labelled enemy-carry sequence exists in this recording; synthetic tracking tests only |

The recorded carry sequence establishes your carry; it does not validate enemy possession.
An ambiguous vase stays `Visible` and produces no ownership claim. Disappearance is not delivery.
Tracking expires across observation gaps. Camp availability, local player position/`your_lane`,
and structure states are not inferred by this implementation.

## Hero ring detection and tracking

Measured on zoomed fixture crops (canonical 335x350 minimap): a hero icon is a portrait inside a
~2.5 px thick ring, centreline radius ~10 px. Allies are cream-white `(242,231,213)`, enemies
red-orange `(224,68,28)`; your own hero is the green-ringed marker.

1. **Mask** each team colour (`ring_masks`).
2. **Circle search** (`hero_rings`): for each candidate centre, 36 rays sample a +-1 px band at
   radii 9/10/11; coverage >= 50% spread over at least three quadrants. Occlusion by towers or other
   portraits is tolerated. An integral image skips empty regions.
3. **Reject furniture:** filled interiors (solid diamonds/blobs); missing diagonal support just
   outside the ring (hollow diamonds only reach the radius on their axes); base arcs; points
   outside the playable ellipse. Near a known structure the interior must look like a portrait,
   not a hollow diamond. Heroes fighting at a tower are therefore kept; the previous detector
   discarded everything within 13 px of a structure and needed 27/32 ring hits.
4. **Explain-away:** accept the strongest ring, claim its band pixels, re-score the rest. Arcs of
   two overlapping portraits no longer form a phantom third hero.
5. **Track** (`Tracker`): `MinimapState.heroes` holds `HeroTrack { ally, pos, unseen_ms, hits,
   velocity, lane }`. Only `hits >= 2` tracks drive coaching. A missing hero keeps its last-seen
   position (dashed in the Map view) until it is forgotten after 45 s.

Before this change the detector found 0 of 5 heroes on `00049` and 2+2 of 5+4 on `00059`.
Cost: ~55-85 ms native per minimap and ~175 ms in WASM; the 1 s fast monitor measured ~0.93 s.

## Positional coaching (`crates/engine/src/positioning.rs`, `data/coaching/positioning.json`)

One call per evaluation, most urgent first, with a source-cited card for each:

| Call | Trigger (all distances are map fractions) |
| --- | --- |
| Gank warning (high) | Confirmed visible enemy within 0.28, outside your fight, closing at >= 0.01/s, and approaching + nearby enemies >= you + nearby allies |
| Outnumbered (high) | >= 2 enemies within 0.12 and more than you + allies there |
| Missing (normal) | Confirmed enemy last seen within 0.28 of you, unseen for 4-30 s, while you are past mid-lane (or 2+ missing) |
| Go in (normal) | Healthy (>= 60%, two reads), 2v1 or a two-hero margin around you, and nobody near you missing |
| Help / join (normal / low) | Healthy, no enemy near you, a teammate 0.12-0.45 away with enemies within 0.09: help if they are even or behind, join if they are ahead |

The lane plan defers to these: during a gank/outnumbered call it holds silently instead of saying
push/poke, and aggressive lane plans are muted while enemies near you are missing.
Thresholds are tunable heuristics, not game facts. Unseen enemies are never assumed safe.

## Coaching behaviour

- A deeply pushed-in lane requires BOTH a confirmed lane-front estimate and nearby enemy-trooper
  evidence before saying to catch that wave ahead of jungling.
- Two pushed lanes with no clearly pushed-in/unknown lane create a *conditional* nearby-camp
  window. This does not claim a camp is available or safe.
- Midboss advice says to gather teammates and prepare lanes; it never directs a solo attempt.
- Urn advice distinguishes bridge availability, personal delivery, and inferred allied/enemy movement.
- Courier delivery takes precedence over wave/camp/shop suggestions.
- Existing priorities and cue cooldowns prevent every frame becoming an announcement.

## Reproduce

From the repository root:

```powershell
cargo test --release --workspace
cargo run --release -p coach-vision --example session -- sessions/1792978351_frames samples/session2-minimap.jsonl
cargo run --release -p coach-brain -- samples/session2-minimap.jsonl
```

The full recording is local/ignored; `samples/minimap/*.png` contains small map-only regression
fixtures. Tests cover portrait rejection, real vase location, relative lane pressure, Midboss icon,
scale handling, expiry, and carrier co-motion. Policy tests exercise wave-versus-jungle conflicts.

Next validation targets: a real enemy-carry sequence, an observed Midboss kill/respawn, and
native-resolution crops during gameplay. Existing recordings only contain compressed crops.
# Active lane-coaching follow-up

The reader now estimates a unique green player ring and projects it onto a lane, withholding
the estimate until consecutive frames support it. Personal HP/max-HP readings and a manual
lane-focus fallback feed a document-grounded decision policy. See [active lane coaching](active-laning.md)
for evidence gates and remaining uncertainty. Older notes below describe the original detector.
