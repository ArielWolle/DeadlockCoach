# Unstable Rift

What the coach reads of the Unstable Rift, what it says, and where the facts come from.
Code: `crates/vision/src/rift.rs` (reads + per-match tracker), `crates/engine/src/rift.rs` (calls,
cards in `data/coaching/rift.json`), `web/src/ai-coach.ts` (`riftLine`, `nextRiftWindow`: the AI
prompt's STATE / Upcoming lines), `web/src/ai-review.ts` (end-of-game digest). Tests:
`crates/vision/tests/rift.rs` (fixtures `samples/frames/00610`-`00619`), the unit tests in
`crates/vision/src/rift.rs`, `crates/engine/tests/rift.rs`, `web/tests/ai-coach.unit.ts`,
`web/tests/ai-review.unit.ts`. Per-frame reads of a recording: `cargo run --release -p coach-vision --example rift_probe -- <img dir> <first> <last>`; before / after call counts: `COACH_NO_RIFT=1` for `active_replay`.

## Facts (deadlock.wiki/Unstable_Rift, re-read 2026-10-30, oldid 159557)

Quoted verbatim in `docs/strategy/midgame-macro.md` section 8 (the cards quote that section):

- In-world effect on a random side lane's bridge -> 60 s -> announcement to all players and the
  Rift on the map -> 20 s -> it opens: a capture point with a 20 m radius around the bridge.
- First effect at 10-12 min (nominal 11:00), later ones 6-8 min after the previous one.
- 12 s to capture; both teams progress while they have a player in the point; claimed only when one
  team is alone in it; the rate does not depend on the player count. Players in the point are
  revealed on the minimap. Reviving (Rejuvenator) or Doorman-banished players do not contest.
- Comeback (linear up to a 15% net-worth gap): the trailing team captures in down to 6 s, the leading
  team in up to 18 s; the trailing team gets bullet / spirit / debuff resist within 35 m
  (10% + 1%/min, max 40% at 30 min).
- Bounty 1482 + 222 per minute from minute 13, split equally. Winners get Rift Troopers in that lane
  (min 5, up to 14 for a trailing winner; +100% health / damage on the first Rift, +20% per later one
  up to 160%; 25% melee and 30-45% spirit resist; no souls). Contested > 50 s: a warning; 60 s: a
  draw, no troopers, the souls drop as 13 Soul Orbs either team can take.
- Settled conflicts: the announcement lead is 60 s (the Cursed Apple page's 40 s is outdated) and
  the interval 7 +/- 1 min (the Strategy page's 8 min is outdated); docs/timings and docs/jungle updated.

## What the screen shows (measured on rec2-rec17, 1920x1080)

| Element | Where / what | Read as |
|---|---|---|
| Announcement | "AN UNSTABLE RIFT IS OPENING..." / "Go to the Rift to claim its power", large slanted cream text in the centre for ~2-3 s (rec17 11:01) | pixel gate (cream share of the centre band), two halves recognised, "UNSTABLE" or "RIFT" + "OPENING" with OCR slack -> `announced` |
| Minimap ping | a bright teal ring (hue 150-176) centred on the bridge for ~5 s right after it (rec17 11:03-11:05; rec16 12:48) | annulus pixels around the two bridges, centroid within 9 px of the bridge (a teammate's ping on a hero beside it, 19 px off, and the Soul Urn's spawn ping on its own bridge, 18-20 px off, are rejected) -> `announced` + side -> lane by the latched minimap layout |
| Capture panel | dark band under the clock: our "33%" left of the teal Rift icon (cream bar growing left), theirs right (red bar), "$ 1482" under the icon, portraits of the heroes in the point; drawn ~8 px lower under a Patron banner (rec17 33:03). Only on screen near the Rift (rec17 19:18-19:44 it was captured with no panel on screen) | gate: teal icon pixels + a near-black track row; the lower position from the icon's top; 4 crops recognised: both percents, the outcome line, the bounty (must be 1482 + 222k) |
| Outcome | "RIFT SECURED!" (teal) / "RIFT LOST" (red) on the panel, then it goes | the words (OCR slack), or the red text (870-1000 px on every lost panel, <= 110 otherwise) with a percent read on the same frame, or one side reaching 100% from 85%+ |
| Draw | never seen in the recordings | only a "DRAW" word on the panel; otherwise a panel that goes without an outcome leaves it unknown |

The Midboss banner can pass the pixel gate (its teal bar); with no percent, bounty or outcome read it
is no Rift (fixture 00617).

## Snapshot: `rift` (crates/core `RiftState`, web `types.ts`)

`phase` announced | open | contested | captured | lost | draw | none, `lane`, `side` (left / right
bridge), `seen_at`, `opened_at` (20 s after the announcement when it was seen, else the first panel),
`ally_progress` / `enemy_progress` and whether each rose since the last read, `contested_secs`,
`bounty`, `captured_by` (ally / enemy), `ended_at`, and `history` (finished Rifts: opening, end,
outcome or null, lane, bounty, `you_there` = your marker at its bridge while it was open). Unread
fields stay null; progress, rates and the contest count are only reported within 12 s of a panel
read. Rules of the tracker:

- A Rift starts on the announcement text, a panel, or a ping inside its window (the first 10:30-13:30;
  later ones 6-8 min +/- 30 s after the last Rift's announcement, or two / three windows later when
  Rifts were missed: rec11's teammate ping on the bridge at 17:41 was rejected, the real ping at
  18:38 accepted).
- Announced, then open 20 s later even without a panel (it is only drawn near the Rift); a Rift never
  seen open is forgotten 75 s after its opening; a seen panel is held 70 s (60 s contest + capture),
  then the Rift closes with the outcome unknown; an outcome is reported for 120 s.
- Contested: both at 95%+, both rising, or one held at the cap while the other rises (rec9 34:00).

## Calls (data/coaching/rift.json; keys: N = the game second the Rift was first seen)

| Key | When | Example (recordings) |
|---|---|---|
| `plan:rift_soon:T` (Low) | 20-40 s before the earliest announcement of the next window, no Rift up | "Rift soon, from 11:00 on a side-lane bridge: push your wave first." (rec9 10:20) |
| `plan:rift:N` (High) | announced (lane known, or 14 s before the opening) or open, all 12 top-bar slots read for the count, not under 40% health | "Rift in Green in 20 seconds: six of us are up, group on the bridge." (rec16 12:48); with a teammate's top-bar position at the bridge "group with Paige on the bridge" (rec6 13:20) |
| `plan:rift:N` (High), behind | our team souls 3%+ under theirs | "We're behind, so we capture the Rift faster: all go Green." (rec16 19:11) |
| `rift:skip:N` (High) | 2+ of us dead and more of ours than theirs | "Skip the Rift: three of us are dead, farm safe." (rec11 33:29, then lost) |
| `plan:rift_stop:N` (High) | open, their % rising, ours flat and under 95% | "They're alone on the Rift: step in to stop the capture." (rec16 13:08: theirs 26% -> 84%, ours 0%) |
| `plan:rift_orbs:N` (High) | contested 48-60 s as observed, or a draw read | "The Rift draws in 10 seconds: grab the soul orbs." (not reached in the recordings) |
| `plan:rift_push:N` (Normal) | within 40 s of "RIFT SECURED!" | "Rift Troopers in Green: push behind them." (rec16 19:33) |
| `rift:defend:N` (Normal) | within 40 s of "RIFT LOST" | "Their Rift Troopers are coming down Yellow: defend at the Guardian, don't fight them alone." (rec10 55:24) |

One announcement call (go / behind / skip) per Rift. The `plan:rift*` calls are go calls (lib.rs
`is_go_call`): never under a hold / back-out call, with 3+ of us dead or while carrying the Urn. The
timed calls are High (20 s to the opening, a 12 s capture, the 60 s draw): in a fight they are cut to
their first clause ("Rift in Green in 20 seconds."); the after-calls use half the phase gap. In the
browser the `rift` and `plan` prefixes are full-scan only (`FULL_ONLY`).

AI coach: the STATE line `Unstable Rift ...` (announced / open with both percents and who is alone
/ contested / taken by us / by them, earlier results) or the next window ("announced between 17:04
and 19:04 (estimate ...)"), the `Upcoming` line (replacing the fixed "first Unstable Rift (about)"
at 11:00), and a live trigger per Rift (`objective-soon:riftN`). End-of-game digest: Rifts seen,
taken by us / them / draws / outcome not read, per Rift lane, bounty and whether you were at the
bridge.

## Validation (fresh replays of rec2-rec17, `replay_dir` + `active_replay --all`)

Rift events seen by eye (contact sheets of the panel and the bridges) and what the vision read:

| Rec | Rift (panel / outcome on screen) | Vision |
|---|---|---|
| rec2 | 13:17-13:26 lost (99/100, 1482); 18:53-19:11 secured (2592) | announced 12:35 Green, panel 13:17, lost 13:26; announced 18:32, panel 18:53 (76%/0), secured frame not read |
| rec4 | 18:52 secured; 26:37 secured; 33:49 lost | captured 18:52; announced 25:52 Yellow -> captured 26:37; announced 33:04 Green -> lost 33:49 |
| rec5 | 19:24 lost; 39:51 lost | lost 19:24; announced 39:07 Yellow -> lost 39:51 |
| rec6 | 13:51 lost; 18:25-18:35 contested 81/64 (end not on screen); 25:59-26:24 lost (4146); 35:14 secured | lost 13:51 (Yellow); contested 18:25, outcome unknown; 25:38 -> lost 26:24; 33:33 -> captured 35:14 (Green) |
| rec8 | 19:22 99/99 (end not seen); 27:02-27:19 secured (4368); 41:01-41:29 lost (7476); 48:14 lost | all four, outcome unknown for 19:22 |
| rec9 | 11:56 lost; 26:09-26:27 lost (4146); 33:47-34:27 99/99 (match end) | lost 11:56; 25:35 Yellow -> lost 26:27; 33:16 -> contested, unknown |
| rec10 | 34:06-34:45 lost (5922); 40:23-40:48 secured (7476); 49:05-49:21 secured (9252); 54:53 lost | lost 34:45; 40:00 Yellow, contested 46/78, end not read; 47:47 Green -> captured 49:17; 54:18 Yellow -> lost 54:53 |
| rec11 | 19:03-19:40 lost (2592); 25:49-25:54 secured (4146); 33:45-34:12 lost (5922) | first Rift announced 12:33 (not on screen); 18:36 Yellow, contested 99/99, "RIFT LOST" frame unreadable; captured 25:54; lost 34:12 |
| rec12 | 12:21 secured; 19:54-20:09 lost (2814, "RIFT LOST" 20:09); 26:55-27:14 secured (4368) | captured 12:21 (Yellow); lost 20:09 (Green); 26:35 Yellow 94/0, secured frame not read |
| rec13 | 19:47 secured; 25:44 secured; 33:39-33:59 secured (5922); 41:30 lost | first announced 11:33; all four with outcomes and lanes |
| rec16 | 13:05-13:24 lost (1482); 19:33 secured; 32:51-33:04 lost (5700) | all three; plus 26:17 announced (Green), not on screen |
| rec17 | 11:24-11:46 secured (1482); 19:44 secured; 25:51 lost; 33:03-33:12 ours 9/59% (match end) | 11:01 text + 11:03 ping Green -> captured 11:46; 18:59 Green, secured frame not read; 25:08 text / 25:11 ping -> lost 25:51; 32:41 -> open 9%/59% |

Before this change the coach said nothing about the Rift in any recording (old replays: no Rift key).
After (rule calls spoken by the engine with its cadence budget and fight mode):

| Rec | Rift calls (game clock) | calls/min before -> after |
|---|---|---|
| rec2 | 10:33 soon; 12:35 behind, all go Green; 18:34 "Rift in Green in 18 seconds." | 1.69 -> 1.74 |
| rec4 | 19:01 push behind Rift Troopers; 24:10 soon; 26:19 / 33:05 behind, all go | 1.93 -> 2.06 |
| rec5 | 19:43 their Rift Troopers, defend | 1.17 -> 1.17 |
| rec6 | 13:20 open in Yellow, group with Paige; 14:18 defend; 19:01 skip (3 dead); 23:32 soon; 25:50 behind; 26:07 they're alone; 26:41 defend; 33:41 group; 35:14 push | 1.27 -> 1.39 |
| rec8 | 19:35 open in Yellow; 26:28 Green in 20 s; 33:48 Green in 15 s; 41:02 skip; 48:42 defend | 1.49 -> 1.62 |
| rec9 | 10:20 soon; 11:30 in 12 s; 12:07 defend; 16:42 soon; 25:35 skip; 33:24 behind | 1.68 -> 1.88 |
| rec10 | 34:06 open; 40:48 open in Yellow; 48:59 behind; 49:06 they're alone; 49:28 push; 54:18 Yellow in 20 s; 55:24 defend | 1.51 -> 1.60 |
| rec11 | 12:35 Green in 18 s; 18:38 behind; 19:05 they're alone (theirs 43% -> 54%, ours 0%); 33:29 skip | 1.64 -> 1.73 |
| rec12 | 11:46 Yellow in 20 s; 12:28 push; 18:56 Green in 10 s; 20:13 defend; 26:39 Yellow in 16 s | 1.18 -> 1.25 |
| rec13 | 11:33 / 18:49 Green in 20 s; 20:23 push; 25:00 Yellow in 20 s; 25:44 push; 33:24 Yellow in 20 s | 1.55 -> 1.68 |
| rec16 | 12:48 Green in 20 s; 13:08 they're alone (lost 13:24); 19:11 behind; 19:33 push; 26:17 / 32:27 skip (3 dead) | 1.59 -> 1.70 |
| rec17 | 11:03 Green in 18 s; 16:30 soon; 18:59 Green in 20 s (fight: first clause); 25:11 Green in 17 s; 30:46 soon | 1.42 -> 1.55 |

The Rift adds 0-0.25 calls/min (1-2 per Rift); the budget before it was already above 1.5 in
several recordings. No Rift call was spoken under a hold / back-out call (the engine guard).

## Limits

- The capture panel is only on screen near the Rift: away from it the phase is "open" with no
  percentages, and the outcome is known only if you saw it (or a later recording frame shows it).
  Several outcome frames are mid-animation and unreadable (rec2 19:11, rec11 19:40, rec17 19:44).
- The lane comes from the announcement ping (or your marker at a bridge while the panel is up) and
  the latched lane layout; without them it is unknown ("on a side lane").
- A draw was never seen: no draw word, colours or timer were measured, so "draw" needs a "DRAW" read;
  the 50 s warning text was not seen either (the orbs call uses the contest it observed).
- The 20 s countdown is the wiki's; announcements seen only by the ping can be up to ~2 s late.
- A teammate's ping exactly on the bridge inside the Rift window would pass as an announcement.
- Portraits of the heroes in the point are not read; who is in the point is inferred from the rates.
- `you_there` needs the Rift's bridge known and two marker reads while the panel was up.
