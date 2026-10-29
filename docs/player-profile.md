# Player profile

The coach learns your habits from the matches it watches (and from replays you add), shows what
you do well and what to fix on the **Player** page, speaks one reminder at a match start or after
a respawn (praise and fixes alternate), and gives the AI coach a short summary of both.

Code: `web/src/player-profile.ts` (pure logic, no DOM), `web/src/player-page.ts` (page),
hooks in `web/src/main.ts` (`playerFrame`, `playerPromptText`, replay collection in `apply` /
`endReplay`). Check it on recordings with
`node --experimental-strip-types web/scripts/player-profile-replay.ts <file.jsonl>...`, or on Replay
lab exports exactly as the browser saw them live (one profile across files, cues included) with
`node --experimental-strip-types web/scripts/player-profile-replay.ts --live <recording.json>...`.

## Privacy

Everything stays in this browser: the profile is stored in `localStorage` under
`deadlock-coach-player-profile-v1` and is never uploaded. Only derived numbers are kept (no
screenshots). Export / Import / Reset are on the Player page; the Settings toggle **Player
profile** (on by default) stops learning and the focus reminders without deleting the data. The
AI coach receives at most the 900-character `promptText()`.

## What is measured per match

The match is the same unit the engine uses: it starts with a readable game clock on a gameplay or
death screen and ends on a new match id with a discontinuous clock, a clock reset (last > 120 s,
now < 60 s, as `crates/web/src/lib.rs`), a wall-clock gap of more than 15 minutes (or more than 3
minutes with an inconsistent clock), or a Victory / Defeat / post-game screen (which also gives the
result). A match id that changes while the clock continues is an OCR misread (rec4 read 160204 and
126120204 in one match). A single backwards clock read is ignored unless it repeats three times.
A match is kept with at least 2 minutes of readable play or one death; at most 30 matches are
stored; a match left open by a closed tab is kept as *partial*.

Live-scan details (rec8-rec12):
- The HUD can keep the previous match's id for minutes (rec12 read rec11's 127139499 until 4:40);
  the record keeps the most-read id, so two matches never share one.
- After a Victory / Defeat / post-game screen, a clocked frame within 2 minutes of the final clock
  is the end-of-game camera, not a new match (rec10: PostGame at 57:55, gameplay at 58:00 started a
  "match" and a focus cue). A clock under 1:00 or a 3-minute wall gap starts the next one.
- Frames without a game clock more than 150 s after the last clocked frame (shop, loading and
  menus between matches: 9 minutes between rec11 and rec12) are not added to the match.

Per-sample reductions are the ones in `crates/engine/src/review.rs` (`sample()`), with the
thresholds of `data/coaching/review.json`:

| Metric | Definition |
| --- | --- |
| Time shares | Each sample stands for the time to the next one, capped at 10 s (wall time live, game clock in replays). Dead = death screen; base = minimap y > 0.86; lane = a lane estimate; jungle = located without a lane. Live full scans almost never locate your minimap marker (rec8: 10 of 478 gameplay frames, rec12: 6 of 424), so without it the zone label under the minimap decides: Lane / EnemyBase = lane, Jungle / Mid = jungle, OwnBase = base; a gameplay frame with neither is *alive, location unknown* (`fieldSecs`). Unknown = anything else (menus, shop, unreadable). Lane / jungle shares use located time; hoarding, low health and enemy-dead windows use all alive time; **not** used for time dead (below). |
| Time dead | See [Time dead](#time-dead): seconds dead / observed game seconds. |
| Deaths | Entering the death screen, or a death screen more than 60 s after the previous one. A single gameplay frame between death screens is merged unless the next timer shows a fresh death. From 10:00 on, a death screen that turns into gameplay within 10 s without a longer timer is the grey "Respawn in: 1" screen, not a death (rec4 26:32, rec8 39:17, rec11 25:17: the scoreboard death count does not change). The damage-panel cause is the latest read. |
| Death context | From the 15 s before the last alive reading (`death_window_secs`): fall speed from the healthiest reading >= 30% (burst = <= 15 s), souls held, alone (all of the last 4 located allies > 0.25 away), in the enemy half (y < 0.48) with >= 3 enemies without a top-bar health bar, teammates who died in the window, seconds since respawn (<= 90 s = soon), every HP read of the last 30 s under 60% (weak entry). Each is `null` when it could not be observed; a death with no readable frames before it is marked "no evidence". |
| Killer family | Victor's Aura of Suffering / Still Life, Drifter's Stalker's Mark and melee hits count as melee range; Bebop's Hyper Beam, structures, troopers, gun damage and unknown abilities (grouped by first word, so OCR variants match) are named but range is not claimed. Optional `death_damage` / `death_killing_blow` are used when the reader provides them. |
| Fights | A minimap cluster of >= 2 allies and >= 2 enemies within 0.13; joined if you were within 0.3 (a third of the map); a cluster absent for less than 10 s is the same fight. |
| Enemy-dead windows | >= 2 enemies dead for >= 20 s while you are alive; *used* when you were in their half (y < 0.5, or the zone label says their base) or the front of the lane you were on moved >= 0.15 toward them; the start of each used window and whether it reached their base are kept. Unknown frames keep a window open without adding time. A top-bar portrait read as dead counts only once its respawn countdown was read (`respawn_secs` / `respawn_observations`): rec12 had Infernus "dead" on most frames from 4:40 on. |
| Soul Urn | Status effect `carrying_urn`: a carry ends when it is gone for > 8 s while alive; *delivered* when no death follows within 10 s, *lost* when you die carrying it. |
| Death-screen shopping | Per death, souls held just before it minus the lowest souls read until the first alive frame (`spentDead`); unsecured souls lost at the death are included (small next to an item). |
| Team lead | Observed ally minus enemy net worth: the first clock of the lead that lasted to the end, the largest lead, the last one. A team result, shown as context. |
| Souls | Alive time over the usual tier for the clock (at least 3,000, `hoard_souls`), its peak, and souls at each death against the death-spend threshold `deathSpend(clock)` = half the usual tier (1,600 / 3,200 / 6,400 before 10:00 / 20:00 / after), at least 1,200 (`souls_at_death`), i.e. 1,200 / 1,600 / 3,200, as `economy.rs death_spend`. The label names that range ("half an item tier or more (1.6k-3.2k by game time)"), never a fixed 1.2k. A missing souls read keeps the previous read for up to 10 s. |
| Low health | Alive time under 30%; episodes under 30% end *survived* (back to >= 50% or in base) or *died*. |
| Deep waves | A wave deep on your side (front < 0.3, >= 2 enemy troopers) while you were in the jungle. |
| Farm pace | Souls per minute from the own earned-souls label, when it was read twice >= 2 minutes apart. |

## Time dead

One metric everywhere (Player page summary and match table, the "Dying too often" trait, the AI
prompt, and the engine's 5-minute summaries "you died N times and were dead for X" in
`crates/engine/src/review.rs` `death_runs()`):

**dead % = seconds dead / observed game seconds.**

- *Observed game seconds* (`MatchRecord.spanSecs`): from the first to the last frame with a game
  clock, every frame-to-frame interval up to 60 s (wall time live, game clock in replays); a longer
  interval is a recording gap and is removed. Shop, scoreboard and unreadable frames count: you
  are alive (or dead) in them, the game goes on. (The engine's summaries use their 5-minute
  window.)
- *Seconds dead* (`MatchRecord.deadSecs`, per death `DeathRecord.deadSecs`): for each death, from
  its first death screen to the respawn. The respawn is the respawn-timer estimate (each read =
  frame clock + timer; the largest group of reads within 2 s, its median; a read refuted by a later
  death screen is dropped), never before the last death screen and never after the first alive
  gameplay frame. Without any timer read it is the first alive gameplay frame, at most 10 s after
  the last death screen. Menus while dead (shop, scoreboard, spectating) stay dead until the
  respawn. Recording gaps inside a death are removed; a match that ends while you are dead counts
  up to its last clock. A summary given while you are still dead counts the rest of the timer.

Why the old numbers were high: the share divided by *readable* time only (lane + jungle + base +
dead), leaving out alive time in the shop, on the scoreboard and on unreadable frames (rec11: 232 s
of 1,976), and each death sample stood for the gap to the next frame, so the frame after the
respawn (often 5-8 s later) and the grey "Respawn in: 1" screens were counted as dead. In the other
direction, spectating frames while dead were "unknown" and the last timer read could be a dropped
digit, which made some engine summaries short (rec4 27:00 said "3 times, 56 seconds" for 2
deaths and 100 s).
Matches stored before this metric (no `spanSecs` / `deadSecs`) are kept in the history, but their
time dead is shown as "— (older version)" and never used.

### Why live said "you spend 90% of the game dead"

The live reminders during rec9-rec12 said 90%, 78%, 65-71% while the respawn timers give 7-23%.
Replaying the live snapshots of rec8-rec12 (Replay lab exports, `--live`) through the version that
was running reproduces the exact numbers: rec8 was stored as 312 s dead of 346 s "readable" (90%),
although 2,271 s were observed. The old share divided by *readable* time (lane + jungle + base +
dead), and a live alive frame was readable only when the minimap marker was located, which the
live full scan did on 10 of 478 gameplay frames: nearly all readable time was death screens. The
next matches averaged in (rec9 67%, rec10 64%, rec11 72%, rec12 85%). The fix that changed the
metric to observed game time was right, but records stored before it kept their old dead time, the
zone label was not used, and short or menu-only records counted. Now: the zone label locates you,
only plausible matches count (below), and old-metric records never feed time dead.

## Which matches count

Every trait, the summary numbers and the spoken reminders use only matches with at least 8
minutes of observed game time, 2 minutes alive in gameplay, and at most 50% of it dead (real
matches: 7-24%). Others stay in the table marked "not counted" with the reason (a few minutes with
one death is not a habit; a death screen or menus is not a match; > 50% dead is a misread).

## Traits and thresholds

The last 10 counted matches are combined with recency weights (newest 1, each older 0.85x) as
weighted sums of numerators and denominators, so a long match weighs more than a short one. Trend compares the newest up-to-3 matches with the older
ones (+/-15%). Confidence grows with the number of events and matches. Severity is how far the
value is over its threshold (>= 1.25x notable, >= 1.6x major). Thresholds are coach heuristics
tuned on the reviewed recordings, not game rules.

| Trait | Weakness when | Strength when | Source |
| --- | --- | --- | --- |
| Dying too often / Stays alive | >= 15% of observed game time dead or >= 2 deaths per 10 min of observed game time | < 7% dead and < 1 per 10 min (>= 15 min) | review.json `pattern_deaths` (>= 60 s dead in 5 min); timer ground truth rec4 24%, rec5 19%, rec8 / rec11 17% (recording-analysis-4: ~25% decided the game) |
| Unspent souls / Spends on time | >= 50% of deaths holding the death-spend threshold for the clock (>= 2 deaths) or >= 20% of alive time over the usual tier | <= 15% and <= 5% | common-mistakes.md #6; review.json `souls`, `pattern_hoard` (60 s of 5 min) |
| Dies alone | >= 2 and >= 35% of deaths | | #16 Fighting split; review.json `alone` |
| Pushes in with enemies missing | >= 2 and >= 30% | | #22 Ignoring the minimap; `forward_missing` |
| Stays in lost fights | >= 2 and >= 30% | | #19; `fight_lost` |
| Dies soon after respawning | >= 2 and >= 35% within 90 s | | #16; `soon_after_respawn` |
| Goes back in hurt | >= 2 and >= 30% | | moba-fundamentals.md; `weak_entry` |
| Gets burst down | >= 2 and >= 60% | | #17 Standing in front of the tank; `burst` |
| Killed at close range | >= 2 and >= 50% of deaths with a range-known cause | | #8, #21; recording-analysis-5 (every death in melee range) |
| Same killer again | one family twice in a match, or 3 times overall | | #8; `repeat_cause` |
| Enemy-dead windows unused / used | >= 1 unused and >= 50% unused | >= 2 used, <= 34% unused | #10; `pattern_window` |
| Misses team fights / Shows up | > 50% missed (>= 4 fights) | <= 25% missed (>= 5) | #25; `pattern_missed` |
| Too much jungle / Keeps lanes covered | jungle >= 50% of alive time, or above lanes and >= 35% | lanes >= 60% (>= 10 min) | jungle-tactics.md (camps between waves); `pattern_jungle`. A low jungle share is never a weakness. |
| Leaves waves deep | >= 60 s per 10 min | | #1; `pattern_waves` |
| Stays in at low health / Backs off | >= 2 and >= 50% of low-health episodes end in death | >= 3 survived, <= 25% died | moba-fundamentals.md "retreat when low" |

Weaknesses are ranked by severity x confidence x specificity (time dead and unspent souls rank
above generic symptoms such as "most deaths were fast"). Practice tips are hero-aware where the
hero's range is unambiguous (Grey Talon, Vindicta: long-range, keep distance; frontline heroes:
save the escape, add a defensive item) and name the repeat killer's hero when the cause is known.

Dying too often is spoken with the games it covers and a target: "Profile focus: you were dead 19%
of your last 3 games; aim under 12%. Stay at max range behind your front line." (time-weighted
share of those games; the threshold uses the recency-weighted value). When only the death rate is
high: "you died 2.1 times per 10 minutes in your last 3 games; aim under 1.5".

## Strengths and improvements

Strengths are first-class, with evidence, like the weaknesses. Three sources:

1. **Last match** (`highlights()`, scope `last`, the newest counted match), concrete plays first:

| Id | Label | When |
| --- | --- | --- |
| `last_windows` / `last_base_push` | Used enemy-dead windows / Pushed their base with enemies dead | >= 1 used and used >= unused (the habit; it silences the unused-windows reminder), else >= 1 into their base (praised as a play) |
| `last_urn` | Delivered the Soul Urn | >= 1 carry delivered |
| `last_death_spend` | Shops on the death screen | >= 1,500 souls spent between a death and its respawn |
| `last_low_dead` | Stayed alive | < 10% of >= 15 minutes dead |
| `last_deathless` | Long deathless stretches | >= 6 minutes alive between deaths (or match start / end) |
| `last_spent` | Spent before dying | >= 2 deaths with souls read, none holding the death-spend threshold |
| `last_lanes` | Kept your lane covered | >= 60% of >= 10 minutes of located alive time on a lane |
| `last_team_lead` | Team lead held | observed team net-worth lead held to the end, peak >= 10k |

2. **Improved since last time** (`improvements()`, scope `improved`): the last counted match
   against the (up to 5) counted matches before it, each with a margin so noise is not praised:
   time dead (`improved_dead`, >= 5 points and <= 75% of before), deaths per 10 min
   (`improved_deaths`, >= 0.5 fewer and <= 70%), enemy-dead windows used (`improved_windows`, +30
   points), deaths with unspent souls (`improved_souls`, -25 points), longest deathless stretch
   (`improved_streak`, >= 6 min, 1.5x and +3 min). The match *clearly improved* when time dead or
   deaths improved, or two others did.
3. **Profile strengths** (several matches, scope `profile`): the table above.

rec12 (Grey Talon, 3 deaths, a win) gives: used 2 of 4 enemy-dead windows (13:38, 27:21), both into
their base; delivered the Urn (16:09-16:33); spent 5.9k souls between the 14:50 death and the
respawn; dead 7% (2 of 29 min); no deaths for 9:17 (0:31-9:48) and 6:43 (15:27-22:10); 86% of
located time on a lane; team lead from 1:10, up to +52k; improved: dead 7% vs 18%, 1.0 vs 1.9
deaths per 10 min. The earlier matches' weaknesses (dying, unspent souls, repeat killer, burst)
are still listed.

## Live reminders

At a match start and at each respawn the coach may say one `player:focus:*` line (Normal
priority), at most every 5 minutes, learned from completed counted matches. Lines alternate
between praise ("Keep it up: last game you used 2 enemy-dead windows to push their base.") and
one fix ("Profile focus: ..."), rotating through each list; the rotation restarts after every new
completed match. After a clearly improved match the first line is its biggest improvement ("Great
game: you were dead 7% of it, down from 18%. Keep it up."). A fix the last match contradicts is
not said (dying after low time dead, unused windows after using them, unspent souls after dying
with none). The cue key is `player:focus:<trait id>`. It goes through the same delivery path as
every other cue (mute, freshness and per-key cooldowns apply).

## API (`PlayerProfile`)

`observe(snapshot, wallMs)`, `endMatch()`, `summary(hero?)` (`strengths` = last-match highlights
then profile strengths, `improvements`, `improved`, `weaknesses`, `rows` with `excluded`),
`promptText(hero?)` (<= 900 characters: "Weak: ...", "Strong: ...", "Improved: ...", "Focus: ...";
ai-coach.ts reads the weakness labels between "Weak:" and "Strong:"), `focusTips(n)` (alternating
praise and fixes), `focusCue()`, `addReplay(snapshots)`, `load()` / `save()` (schema 1; a bare
array is migrated; newer schemas are refused; new fields are optional), `exportJSON()` /
`importJSON()`, `reset()`.
The AI coach gets the text through `setProfileProvider(playerPromptText)` (exported by `main.ts`),
or `window.deadlockPlayerProfile.promptText()`.

## Validation on the reviewed recordings

### Time dead against the respawn timers

Ground truth per match: each death from its first death screen to the respawn read off the
"Respawn in: N" timer (checked on the screenshots `frames/recN/img`, e.g. rec11 25:28 shows 54),
capped by the first alive frame; observed time = first to last clocked frame minus recording gaps
> 60 s. Newest-pipeline replays, profile with recorded wall time (clock-only replays agree within
1 s, except rec5, which a re-read match id splits at its 7-minute gap):

| Match | Truth: deaths, dead / observed | Truth % | Before | After (profile) |
| --- | --- | --- | --- | --- |
| rec4 `replay9` (Grey Talon) | 5, 312 / 1,319 s | 23.7% | 22% (189 s of 873 readable; spectating while dead was unknown) | 24% (313 / 1,319 s) |
| rec5 `replay7` (Grey Talon) | 4, 241 / 1,250 s (447 s gap) | 19.3% | 21% (237 of 1,105 readable) | 19% (242 / 1,250 s) |
| rec8 `replay2` (Grey Talon) | 6, 395 / 2,271 s | 17.4% | 18%, 7 deaths (grey screen at 39:17 counted) | 17% (393 / 2,271 s), 6 deaths |
| rec11 `replay1` (Violet) | 7, 339 / 1,976 s | 17.2% | 20% (344 of 1,743 readable), "mostly artist" | 17% (339 / 1,976 s), "mostly Violet" |

Per death, rec11: 16 / 22 / 33 / 53 / 61 / 75 / 79 s (truth 16 / 22 / 33 / 54 / 61 / 75 / 78). The
engine's 5-minute summaries on the same replays (truth in brackets): rec11 12:24 "2 times, 36
seconds" (38), 28:08 "2 times, 1.9 minutes" (1.9; before: the grey screen made it 2 deaths of 59 s
at 25:49), 37:32 "2.6 minutes" (2.6); rec8 21:30 "1.7 minutes" (1.7; before 84 s), 50:26 "2.5
minutes" (2.5); rec4 27:00 "2 times, 1.7 minutes" (2, 1.7; before "3 times, 56 seconds"), 31:12
"3 times, 2.4 minutes" (3, 3.2: the timer of the death in progress was read 8 / 5 / 4 / 2 / 0 for
68-60, so its rest is unknown until a correct read); rec5 39:09 "78 seconds" (78). The unit test
`time dead on the newest-pipeline recordings matches the respawn-timer ground truth` checks the
profile numbers when the recordings are present; `crates/engine/tests/review.rs` covers the
summary cases.

### Habits on the reviewed recordings

`rec4/replay9.jsonl` then `rec5/replay4.jsonl` (Grey Talon), with recorded wall time:

| | rec4 | rec5 | human review |
| --- | --- | --- | --- |
| Deaths | 5 | 4 | 5 / 4 |
| Time dead | 24% | 19% | ~25% / 17-21% |
| Unspent at death (death-spend threshold) | 0 of 5 (1.4k-3k, thresholds 1.6k-3.2k) | 1 of 4 | every rec4 death 1.4k-3k |
| Over 3k, alive time | 26% | 44% (peak 6.3k) | 30% / ~43% |
| Killers | melee x2, Hyper Beam, Chimera | Victor's aura x2, Drifter, gun damage | rec5: every death in melee range (Victor, Drifter) |
| Enemy-dead windows unused | 2 (20:28, 32:07) | 1 (38:21) | 20:24, 32:07 / 37:24-38:38 |
| Jungle (alive) | 15% | 17% | 7-17% / ~11% (not a problem) |
| Fights joined | 8 of 11 | 11 of 16 | 9 of 14 / 7 of 12 |

Weaknesses produced: killed at close range (5 of 6), dying too often (21% of 43 observed minutes,
2.1 per 10 min), dies soon after respawning, enemy-dead windows unused (3 of 3), gets burst down, stays in
at low health, same killer again (Victor x2, melee x2). Strength: keeps lanes covered (76% lanes,
16% jungle). No "too much jungle". The unit test `player profile on the reviewed recordings
matches the human reviews` checks this when the recordings are present (with and without wall
time).

### Live snapshots, rec8-rec12 in sequence

The Replay lab exports `frames/rec8..rec12/recording.json` (live full-scan snapshots with wall time)
fed as the browser does (`player-profile-replay.ts --live`, one profile, cues at match starts and
respawns). Before = the version that was live (readable-time metric), after = this one:

| Match | Truth (timers) | Before | After |
| --- | --- | --- | --- |
| rec8 Grey Talon 13:25-51:16 | 17% | 90% (312 / 346 s "readable") | 17% (393 / 2,271 s) |
| rec9 Grey Talon 9:02-34:37 | | 67% | 17% (261 / 1,535 s) |
| rec10 Violet 34:04-57:55 | | 64% | 23% (327 / 1,431 s) |
| rec11 Violet 6:13-39:09 | 17% | 72% | 17% (339 / 1,976 s) |
| rec12 Grey Talon 0:31-28:49 | ~7-8% | 85% | 7% (121 / 1,732 s), id 127160441 (not rec11's) |

Spoken before: "you spend 90% / 78% / 71% of the game dead" on every reminder from rec9 on. After:
rec11 20:15 "you were dead 19% of your last 3 games; aim under 12%", alternating with praise such
as rec11 12:19 "Keep it up: last game you used 6 of 6 enemy-dead windows to push, twice into their
base." and rec12 23:24 "Keep it up: last game you spent 3.2k souls on the death screen and came
back stronger."; after rec12 the next reminder is "Great game: you were dead 7% of it, down from
18%. Keep it up." The unit test `live recordings rec8-rec12 (browser path)` checks this when the
recordings are present.
