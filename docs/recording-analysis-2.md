# Recorded match analysis 2

A 1920x1080 match recorded in the browser app: 600 frames, game clock 2:11-19:23, 55 spoken
cues. An independent reviewer rated the spoken tips from the exported data alone: 24% helpful,
51% generic, 11% mistimed, 13% unsupported by the data, 2% wrong. Fixes were made data first,
then callouts, and measured by replaying the same 600 frames (`tools/recording`).

## Data

| Measure (same 600 frames) | Live | After fixes |
| --- | --- | --- |
| Frames classified Gameplay / Unknown | 465 / 135 | 574 / 25 (the rest: heavy effects, Tab overlay) |
| Clock jumps (backward, or ahead of wall time) | 7 (e.g. 4:20 for 17:00) | 0 |
| Midboss "up" flag flips | 77 | 7 |
| Own HP read | 416 frames | 572 (32 under 15%, was 24) |
| Own hero identified | never | 534 frames (Deadman Danny; Paige early while portraits settle) |
| Minimap hero track ids (11 heroes seen) | 140 | 34 |
| Spurious items (seen < 10 frames) | 14 kinds, incl. 6,400 at 2:59 | 1 |
| Trooper count jitter (sum of frame-to-frame changes) | 757 | 300 |
| Team net worth drops > 10% | 1 (caused a false "behind") | 0 |

- **Unknown screens.** When the classifier abstains but the clock was confirmed within two minutes,
  the own health bar means Gameplay and the respawn text means Dead.
- **Clock.** A verified reading that contradicts the wall-time extrapolation needs consistent
  repeats (3 forward, 2 backward); a clock standing still on the last confirmed value (pause,
  stalled stream) is adopted at once. Small backward misreads (8:11 read as 8:01) are rejected.
- **Own HP at low health.** The red damage vignette hides the digits exactly when it matters; the
  slanted health bar still measures within ~0.03, so it supplies HP when the digits are unreadable
  (full scan and fast monitor). The HP/bar agreement tolerance scales with the bar (0.06 + 0.15 x bar).
- **Own hero.** You are one of the six ally top-bar slots: the one whose health bar follows your HP
  (mean gap 0.035 for the right slot, >= 0.25 for every other). Own net worth comes from that slot.
- **Items.** Accepted only when the same match repeats on two scans in a slot, the item is in the
  shop list (tier-5 placeholders excluded), and it costs no more than 1,500 + 1,200 souls per minute.
- **Minimap.** Midboss state is event-based (gone 20 s = killed, seen again = back); urn-shaped icons
  on both bridges are power-ups; with a whole team tracked, an unexplained ring continues the nearest
  unmatched track; trooper counts are the median of the last three reads; the lane front is the
  split that best explains the lane as "ours, then theirs" (local occluders cannot end it early).
- **Team totals.** A drop of more than 10% needs ten consistent reads; a hero above 45% of the team
  total is dropped.
- rec2 contains no own death: both near-deaths were retreats to the fountain (clock continuous, HP
  refilled in 2-4 s).

## Callouts

- **Positioning and jungle calls were never spoken in the browser.** The web engines did not
  register `PositionCoach` / `JungleCoach`, while the lane plan muted itself in their favour. Both
  are now registered in the full and fast engines. Full-scan-only cues (net worth lead/behind,
  counters, death cause) are no longer dropped while the fast monitor is live.
- **Critical health (< 15%)** speaks on a single bar-checked reading and preempts the cue gap
  (rec2: silent at 6%; the replay now calls it at 5%, 6% and 12%). Low but clearly rising health
  (fountain) no longer says "back off".
- **"Back off before trading" is muted** while three or more enemies are dead on the top bar and none
  is visible near your confirmed position (unknown position keeps the warning).
- **Lead/behind** is not called when one team reads under half the other (a digit drop).
- **Counter items** need the souls actually read and must fit the observed build (no spirit item once
  you own three weapon items and no spirit item, and vice versa).
- **Wave check** speaks only when a lane needs action (pushed out or coming toward you), at most every
  45 s, and never repeats the same report within 5 minutes (rec2: 10 -> 2).
- **Midboss** "icon is lit" is said once per match; "back on the map" is said when it returns.

## Known limits

- Early portrait identities can swap between similar slots for the first minutes.
- Kill feed, cooldowns, enemy items outside the scoreboard, objective health and unsecured souls are
  still not read.
