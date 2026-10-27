# Recorded match analysis 1

A 1920x1080 match recorded in the browser app. The recorder kept the last 120 analyzed frames
(game clock 2:00-4:05; the cap is now 600 frames). User notes: "health low" too often, jumpy
match state, random wave checks. Method: per-field statistics of the live snapshots, frame
contact sheets against the screenshots as ground truth, and a deterministic replay
(`tools/recording`) of the same 120 frames after each fix.

| Measure (same 120 frames) | Live | After fixes |
| --- | --- | --- |
| Own HP readings below 45% | 23 (20 were a misread) | 4 (a real fight) |
| Player marker found | 34 | 103 of 103 gameplay frames |
| Green / Yellow lane front | 0-3% / 0-15% ("pushed to base") | 31-56% / 39-52% |
| Top-bar seen/unseen flips (12 slots) | 129 | 45 |
| Slot-frames marked dead | 126 | 15 (exactly the two real deaths) |
| Impossible souls values (> 2000) | 1 | 0 |

## Root causes

- **Truncated HP number.** The tracked HP box was acquired at a two-digit value; when HP regenerated
  to 794 it kept reading "79" (consistent, so trusted). Fix: the large slanted health bar is
  measured from pixels (within ~0.03 of correct OCR on every frame) and must agree within 15
  points, otherwise the boxes are re-acquired. Fresh >30% jumps need the bar or a repeat frame.
  The sudden-damage trigger now needs a 15% drop between reads (was 8%); low health needs two reads.
- **Full top-bar bars read as missing.** The bar-shape check ran over only the edge rows above a
  full fill; the 1080p capture also shows a dim-red gradient at the top of enemy fills. Fixes:
  skip the check for near-full bars, accept the dim fill (not the darker, more saturated CRITICAL
  glow or orange portrait art), and require **two consecutive agreeing frames** for any seen /
  unseen / dead change.
- **Pale portraits read as dead.** Death now also requires the red respawn timer (78-216 timer
  pixels on every true death, 0 on pale living heroes).
- **Lane colours swap between teams.** Half of the matches have Yellow on the left. Lane fronts,
  trooper counts and your lane were computed with the wrong lane's colour. The layout is now
  detected from the lane lines each frame (`MinimapState.lanes_mirrored`).
- **Player marker missed.** Its ring measured saturation 0.49-0.58 (below the generic mask) and was
  discarded next to towers. It now has its own mask, the occlusion-tolerant ring search, and an
  exclusion list for the mint shop icons instead of a structure filter.
- **Frames dropped as Unknown when the small SOULS label was unreadable.** A landmark-verified
  minimap now forms the game context alone; souls are left unknown in that case.
- **`$` read as a digit** ("$717" -> 3717, "$1,065" -> 31065). The digit glyphs right of the tall `$`
  are counted from the mint text mask (merged glyphs counted by width) and an invented leading
  digit is removed.

The live export had no spoken cues because the fast path speaks them and is not stored as frames;
exports now include every spoken cue. Regressions: `crates/vision/tests/recording.rs` with five
frames from this match (`samples/frames/005xx_rec-*`).

Not yet validated: whether gank/missing/help calls were correct at the moment they fired (needs a
longer recording with spoken cues and user-noted moments).
