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
| Hero visibility | Circular red/white support, nonmaximum suppression | Overlapping rings can be missed. Zero detections does not establish that enemies are absent or dead |
| Midboss | Gold winged icon near centre, supported in at least two of three frames | Positive availability evidence only. Absence never establishes a kill or starts a respawn timer |
| Urn marker | Cyan cap/body shape, furniture exclusions, consecutive spatial support | Cyan portraits, delivery outlines, and occlusion can still cause misses |
| Urn on ground | Stationary confirmed vase at a bridge spawn position | Do not infer ground state elsewhere |
| Your carry | Literal `DELIVER THE URN` HUD text plus confirmed vase | Disabled while spectating |
| Ally/enemy carry | Vase and same-team ring moving together over two frames | Heuristic. No labelled enemy-carry sequence exists in this recording; synthetic tracking tests only |

The recorded carry sequence establishes your carry; it does not validate enemy possession.
An ambiguous vase stays `Visible` and produces no ownership claim. Disappearance is not delivery.
Tracking expires across observation gaps. Camp availability, local player position/`your_lane`,
and structure states are not inferred by this implementation.

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
