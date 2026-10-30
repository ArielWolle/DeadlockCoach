# Coach review rec16 / rec17: who is alive and who is dead

Two Grey Talon games recorded with the browser coach: rec16 (loss, 12:48-38:02, Grey Talon in
ally slot 4) and rec17 (win, 10:46-32:53, ally slot 0), 600 frames each. The player reported that
the coach was sometimes wrong about which teammates and enemies were alive or dead. This review
checks every spoken call that depends on dead/alive counts against the screenshots, finds the
root causes, fixes them and measures the result again.

## Method

- **Ground truth.** I cropped the top bar (6 ally + 6 enemy portraits and the clock) into contact
  sheets and read 100 frames by hand: every frame where a dead/alive call was spoken live (fast,
  full and AI) or by the current-main replay (`cues-base.txt`), plus 58 frames sampled every 20
  frames. A dead hero's portrait is greyed out and shows its respawn countdown in red-orange
  digits. Every other slot is alive. For each frame I recorded each side's respawn timers.
- **Slot-level truth.** A strict digit-colour pixel count (hue 4-24, saturation > 0.78,
  value > 0.75 on the digit rows) gave exactly my hand counts on all 100 frames, so it marks
  which slot each timer is in. The same count serves as a proxy for all 1,200 frames.
- **Calls.** Each call is matched to its game by wall time (`spoken[].t` between the first and
  last frame), not by clock, because `spoken[]` covers several games. Each call is graded
  against the frame it was spoken on: correct, wrong count, stale, should not have been said, or
  counted the player himself.

## Accuracy of `hero_status[].dead`

| Source | 100 checked frames: both teams' dead counts right | Slot reads right | False "dead" | Missed dead | Timers read (when dead) right |
| --- | --- | --- | --- | --- | --- |
| Live snapshot (browser) | 59 / 97 (61%) | 1,116 / 1,164 (95.9%) | 48 | 0 | 225 / 225 |
| Current-main replay | 61 / 96 (64%) | 1,114 / 1,152 (96.7%) | 38 | 0 | 217 / 217 |
| **This branch (replay)** | **97 / 97 (100%)** | **1,164 / 1,164 (100%)** | **0** | **0** | 221 / 221 |

All 1,200 frames, measured against the digit proxy (frames with a 12-slot read):

| Source | Frames exact | Slot accuracy | False dead | Missed dead |
| --- | --- | --- | --- | --- |
| Live | 584 / 1,046 (55.8%) | 95.56% | 552 | 5 |
| Current-main replay | 667 / 1,039 (64.2%) | 96.79% | 396 | 4 |
| This branch | 1,047 / 1,050 (99.7%) | 99.98% | 0 | 3 (first frame of a death) |

Frames where the minimap was not found report no top bar at all (`hero_status` is empty), so
every count-based rule stays silent there. In the checked set that was 3 live frames and 3 frames
on this branch.

Every error was a false death ("phantom"), and almost every phantom had no respawn timer read
(`respawn_secs` null).

## Graded calls

### Spoken live (both games, fast + full + AI)

| Game | Clock | Source | Call (short) | Truth on the top bar | Verdict |
| --- | --- | --- | --- | --- | --- |
| 16 | 14:02 | full | Hold in base | 3 allies dead (20/23/25 s) | correct |
| 16 | 15:51 | full | "Back out: Silver and Shiv are down" | no ally dead | **should not have been said** (phantom) |
| 16 | 17:01 | fast | 2 teammates dead, first back in ~14 s | 2 dead, 10 / 26 s | **stale timer** (4 s old) |
| 16 | 17:22 | full | two teammates respawn within 14 s | 14 / 4 s | correct |
| 16 | 18:11 | ai | "Silver's back in 2s ... while two stay dead" | no ally dead; 4 enemies dead, 3 stay | **wrong count** |
| 16 | 20:42 | full | we're losing at Blue lane | 3 allies vs 0 dead | correct |
| 16 | 20:55 | full | urn hold: three teammates dead | 3 | correct |
| 16 | 21:00 | ai | three of ours are dead | 4 (one more died 1 s earlier) | **stale** |
| 16 | 22:05 | ai | two of theirs dead | 2 (36 / 31 s) | correct |
| 16 | 23:55 | ai | Seven and Harrow dead for 30 s | 2 dead, 15 / 49 s | **wrong timer** (model) |
| 16 | 24:12 | full | death review: Silver and Paige fell | both dead | correct |
| 16 | 29:15 | full | urn: 2 of them dead | 2 | correct |
| 16 | 30:07 | fast | 3 teammates dead, first back in ~28 s | 3, first 25 s | **stale timer** |
| 16 | 30:34 | ai | "Paige is back in 5 ... Venator and Harrow dead" | Paige alive (prompt: Silver, Shiv, Mina, Paige dead); only Shiv dead | **should not have been said** (phantoms) |
| 16 | 32:02 | fast | 2 teammates dead, first back in ~65 s | 1 dead (61 s) | **wrong count** (phantom Shiv) |
| 16 | 32:48 | ai | three teammates down when you died | 3 + you | correct |
| 16 | 33:50 | full | Hold in base | 4 dead | correct |
| 16 | 34:09 | ai | "wait with our living ally" | 5 teammates dead; prompt "Allies: 1 alive" was the player | **counted the player** |
| 16 | 34:59 | ai | "two are dead" | 3 dead (prompt STATE 3, WHY NOW 2) | **wrong count** (stale WHY NOW) |
| 16 | 37:06 | ai | four of ours down when you died | 2 teammates (Mina, Billy) | **wrong count** (phantoms Shiv, Paige) |
| 17 | 11:58 | ai | three of theirs are dead | 1 (8 s) | **wrong count** (phantom Shiv) |
| 17 | 13:36 | ai | their window closes in seconds | none dead (prompt: Wraith 4 s) | **stale** |
| 17 | 14:15 | full | urn: 2 of them dead | 1 (Celeste) | **wrong count** (phantom Shiv) |
| 17 | 15:43 | ai | two of theirs are dead | 1 (Paige 24 s) | **wrong count** (phantom Shiv) |
| 17 | 21:35 | ai | "Shiv's down" | nobody dead | **should not have been said** |
| 17 | 24:06 | ai | "Shiv's dead" | nobody dead | **should not have been said** |
| 17 | 24:59 | ai | three of theirs dead, Paige back in 11 | 3 dead, Paige 3 s | **stale timer** |
| 17 | 26:58 | ai | their window is closing | 2 dead, one at 1 s | correct |
| 17 | 31:26 | full | midboss: three dead for 34 s | 49 / 34 / 70 | correct |
| 17 | 32:05 | full | out of their base: Shiv and Wraith back in 10 s | Shiv 10 s, Wraith back already | correct |

Before (live): **12 correct / 30**. The other 18: 7 wrong counts, 1 that counted the player, 4
that should not have been said (phantom deaths), 5 stale and 1 wrong timer.

### Replay rule calls (current main vs this branch)

| Game | Current main (`cues-base.txt`) | Verdict | This branch | Verdict |
| --- | --- | --- | --- | --- |
| 16 | 14:13 go back to base (3 dead) | correct | same | correct |
| 16 | 17:36 go back to base | **should not** (1 dead; phantoms 1, 2) | not said | correct |
| 16 | - | - | 17:54 two dead for 39 more s (4 dead: 21/39/41/27) | correct |
| 16 | 19:25 urn: 3 of them dead | **wrong count** (2; phantom enemy 4) | "2 of them dead" | correct |
| 16 | 20:43 team losing / 20:55 urn hold (3 dead) | correct | same | correct |
| 16 | 22:16 two dead for 20 s | correct | same | correct |
| 16 | 24:12 Silver and Paige fell | correct | 24:08 same | correct |
| 16 | - | - | 25:41 2 teammates dead, first back ~24 s (24 / 38) | correct |
| 16 | 26:48 hold in base: three dead, next 14 s | correct | same | correct |
| 16 | 30:05 3 teammates dead, ~25 s | correct | same | correct |
| 16 | 32:23 3 teammates dead, ~39 s | correct | not said (cadence) | - |
| 16 | 33:50 hold in base | correct | same | correct |
| 16 | - | - | 32:44 death review: Shiv fell and you stayed in (Shiv died 32:23) | correct |
| 16 | 35:27 two dead for 33 s | correct | 35:24 two dead for 36 s (36 / 1 / 44) | correct |
| 17 | 12:46 losing at Yellow (2 vs 1) | correct | same | correct |
| 17 | 15:44 two dead for 24 s | **wrong count** (1; phantom Shiv) | not said | correct |
| 17 | 23:03 five dead for 19 s | correct | same | correct |
| 17 | - | - | 24:48 three dead for 15 s (31 / 15 / 38) | correct |
| 17 | 25:14, 32:04 forward exits | correct | same | correct |

Calls that depend on dead/alive state, current main: 14 correct, 3 wrong (2 wrong counts, 1
should not have been said). This branch: **18 correct, 0 wrong**. The one correct main call this
branch no longer makes (32:23) was skipped by cadence, not because of a misread.

AI and fast-path calls cannot be replayed offline. On this branch, every AI and fast call frame
above now has the right top-bar state (they are in the 100-frame set), and the prompt and
fast-path fixes below are unit-tested.

## Error classes and fixes

1. **Red portrait art read as a respawn timer** (most phantoms). Shiv's portrait has about 95
   red-orange pixels on every frame. When his bar was not drawn (an enemy not seen recently, or
   a slid crop), the old test (≥ 55 loose red pixels, no CRITICAL glow) marked him dead:
   - rec17 enemy 3 was dead without a timer for most of the game. This caused the AI's "Shiv's
     dead / Shiv's down", "two/three of theirs are dead", the urn plan's "2 of them dead" and
     `pos:enemies_down` at 15:44.
   - Ally Shiv in rec16 caused the fast path's "2 teammates are dead" at 32:02 and the AI's
     "four of ours were down".

   Fix (`crates/vision/src/team.rs`, `digit_glyph`): death now rests on the countdown digits
   alone. A slot is dead when it has at least 30 vivid, bright digit pixels (hue 3-25,
   saturation > 0.78, value > 0.8) spread over at least 8 rows of the digit band. On the
   recordings, digit pixels measured saturation ≈ 0.86 and value ≈ 0.89; Shiv's art measured
   0.66-0.76 and 0.71-0.75. Over 14,400 slot-frames, living slots have 0 such pixels (3 had 10-39)
   and dead slots have 45-250, even for a single "1". A count of 12-29 is "unsure" and keeps the
   previous state. A drawn bar overrules a weak glyph.

2. **Game-context jitter slid every top-bar crop.** The game rectangle is derived from the
   minimap's landmark box. On a full-screen 1920x1080 capture it came out as e.g. 1910x1074 at
   (8,5) or 1891x1064 at (23,15), which moved the portraits by up to 19 canonical pixels. Bars
   went "unseen" and pale or dark living portraits (Silver, Shiv, Billy) read dead. Examples:
   rec16 13:29-13:37 and the death screen at 24:12, where allies 1 and 2 read dead and so did the
   15:51 "Silver and Shiv are down". Live and replay differ here because their contexts jitter
   differently.

   Fix (`auto::snap_full_frame`): a derived context within 3.5% of a 16:9 frame is the whole
   frame. The quick monitor also aligns the top bar (`top_bar_offset`) like the full scan does.

3. **Respawn lag.** A respawned enemy stays without a bar, so the switch back to alive waited
   for a second frame. One frame late, rec16 19:25 said "3 of them dead" with 2 dead.

   Fix: when a countdown about to end (≤ 5 s left, counted from its anchor at frame time,
   `apply_bars_at`) loses its digits, the slot commits alive at once.

4. **Static numbers accepted as a countdown.** Two equal reads of art 1-2 s apart passed the
   countdown check (the replay read "29"/"31" on Shiv's living portrait).

   Fix: over 2 s or more, the second read must be lower.

5. **Fast path used old timers.** The fast monitor copied the full scan's `respawn_secs` and
   kept them unchanged ("first back in about 14 s" for 10, "28" for 25).

   Fix: `team::age_respawn` counts them down by the context's age.

6. **Unread slots counted as alive.** `TeamPicture` and `dead_set` treated `dead: None` as
   alive.

   Fix: they need all twelve slots read; otherwise every count-based rule stays silent
   (`positioning::top_bar_read`).

7. **AI prompt: the player counted as an ally.** "Allies: 1 alive" during a 5-teammate wipe was
   the player himself, and the reply was "wait with our living ally". The death prompt also
   listed Grey Talon among the dead allies.

   Fix: the line now reads "Teammates (not counting you): N alive, dead: ..., not read: N". An
   unread slot is never counted alive.

8. **AI prompt: stale WHY NOW counts.** The enemy-dead event kept the count from when it began.
   At rec16 34:56, WHY NOW said "2 enemies dead" three times while STATE said 3, and the reply
   said "two are dead". At 13:36 in rec17 the window had already closed.

   Fix: enemy-window and allies-down reasons are restated from the latest snapshot, and dropped
   when the window has closed.

## Regression fixtures and tests

- `samples/frames/00590`-`00599`: eight top-bar strips and two full frames with a jittered
  context.
- `crates/vision/tests/alive_rec16_17.rs`:
  - only slots that show digits read dead (Shiv alive and unseen, single "2" and "1", 2-5 dead
    slots);
  - the respawn digits read 2 / 3 / 75, 1 / 37 / 9 and 73 / 60 / 7 / 78;
  - `snap_full_frame` on the measured contexts;
  - the automatic scanner on the jittered frames: nobody dead at 13:32, the exact five dead at
    24:12;
  - a static number is no countdown;
  - fast-path timer aging.
- `crates/engine/tests/positioning.rs` `an_unread_top_bar_slot_silences_every_count`.
- `web/tests/ai-coach.unit.ts` "top bar in the prompt": teammates exclude you, an unread slot is
  not alive, and WHY NOW restates the current count and drops a closed window.

Tools: `crates/vision/examples/alive_survey.rs` (per-slot digit, timer and bar features and the
stateful dead set for a directory). `replay_dir` now also writes the game context per frame.
