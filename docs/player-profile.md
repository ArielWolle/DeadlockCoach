# Player profile

The coach learns your habits from the matches it watches (and from replays you add), shows them
on the **Player** page, reminds you of one focus point at a match start or after a respawn, and
gives the AI coach a short summary of them.

Code: `web/src/player-profile.ts` (pure logic, no DOM), `web/src/player-page.ts` (page),
hooks in `web/src/main.ts` (`playerFrame`, `playerPromptText`, replay collection in `apply` /
`endReplay`). Check it on recordings with
`node --experimental-strip-types web/scripts/player-profile-replay.ts <file.jsonl>...`.

## Privacy

Everything stays in this browser: the profile is stored in `localStorage` under
`deadlock-coach-player-profile-v1` and is never uploaded. Only derived numbers are kept (no
screenshots). Export / Import / Reset are on the Player page; the Settings toggle **Player
profile** (on by default) stops learning and the focus reminders without deleting the data. The
AI coach receives at most the 600-character `promptText()`.

## What is measured per match

The match is the same unit the engine uses: it starts with a readable game clock on a gameplay or
death screen and ends on a new match id with a discontinuous clock, a clock reset (last > 120 s,
now < 60 s, as `crates/web/src/lib.rs`), a wall-clock gap of more than 15 minutes (or more than 3
minutes with an inconsistent clock), or a Victory / Defeat / post-game screen (which also gives the
result). A match id that changes while the clock continues is an OCR misread (rec4 read 160204 and
126120204 in one match). A single backwards clock read is ignored unless it repeats three times.
A match is kept with at least 2 minutes of readable play or one death; at most 30 matches are
stored; a match left open by a closed tab is kept as *partial*.

Per-sample reductions are the ones in `crates/engine/src/review.rs` (`sample()`), with the
thresholds of `data/coaching/review.json`:

| Metric | Definition |
| --- | --- |
| Time shares | Each sample stands for the time to the next one, capped at 10 s (wall time live, game clock in replays). Dead = death screen; base = minimap y > 0.86; lane = a lane estimate; jungle = located without a lane; unknown = anything unreadable. Unknown time is excluded from every share. |
| Deaths | Entering the death screen, or a death screen more than 60 s after the previous one. A single gameplay frame between death screens is merged (rec4 26:37). The damage-panel cause is the latest read. |
| Death context | From the 15 s before the last alive reading (`death_window_secs`): fall speed from the healthiest reading >= 30% (burst = <= 15 s), souls held, alone (all of the last 4 located allies > 0.25 away), in the enemy half (y < 0.48) with >= 3 enemies without a top-bar health bar, teammates who died in the window, seconds since respawn (<= 90 s = soon), every HP read of the last 30 s under 60% (weak entry). Each is `null` when it could not be observed; a death with no readable frames before it is marked "no evidence". |
| Killer family | Victor's Aura of Suffering / Still Life, Drifter's Stalker's Mark and melee hits count as melee range; Bebop's Hyper Beam, structures, troopers, gun damage and unknown abilities (grouped by first word, so OCR variants match) are named but range is not claimed. Optional `death_damage` / `death_killing_blow` are used when the reader provides them. |
| Fights | A minimap cluster of >= 2 allies and >= 2 enemies within 0.13; joined if you were within 0.3 (a third of the map); a cluster absent for less than 10 s is the same fight. |
| Enemy-dead windows | >= 2 enemies dead for >= 20 s while you are alive; *used* when you were in their half (y < 0.5) or the front of the lane you were on moved >= 0.15 toward them. Unknown frames keep a window open without adding time. |
| Souls | Alive time over 3,000 unspent (`hoard_souls`), its peak, and souls at each death (`souls_at_death` 1,500). A missing souls read keeps the previous read for up to 10 s. |
| Low health | Alive time under 30%; episodes under 30% end *survived* (back to >= 50% or in base) or *died*. |
| Deep waves | A wave deep on your side (front < 0.3, >= 2 enemy troopers) while you were in the jungle. |
| Farm pace | Souls per minute from the own earned-souls label, when it was read twice >= 2 minutes apart. |

## Traits and thresholds

The last 10 matches are combined with recency weights (newest 1, each older 0.85x) as weighted
sums of numerators and denominators. Trend compares the newest up-to-3 matches with the older
ones (+/-15%). Confidence grows with the number of events and matches. Severity is how far the
value is over its threshold (>= 1.25x notable, >= 1.6x major). Thresholds are coach heuristics
tuned on the reviewed recordings, not game rules.

| Trait | Weakness when | Strength when | Source |
| --- | --- | --- | --- |
| Dying too often / Stays alive | >= 15% of readable time dead or >= 2 deaths per 10 min | < 7% dead and < 1 per 10 min (>= 15 min) | review.json `pattern_deaths` (>= 60 s dead in 5 min); recording-analysis-4 (~25% dead decided the game), -5 (17-21%) |
| Unspent souls / Spends on time | >= 50% of deaths holding 1,500+ (>= 2 deaths) or >= 20% of alive time over 3,000 | <= 15% and <= 5% | common-mistakes.md #6; review.json `souls`, `pattern_hoard` (60 s of 5 min) |
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

## Live reminders

At a match start and at each respawn the coach may say one `player:focus:*` line (Normal
priority) for the top weakness learned from completed matches, at most every 5 minutes. It goes
through the same delivery path as every other cue (mute, freshness and per-key cooldowns apply).

## API (`PlayerProfile`)

`observe(snapshot, wallMs)`, `endMatch()`, `summary(hero?)`, `promptText(hero?)` (<= 600
characters), `focusTips(n)`, `focusCue()`, `addReplay(snapshots)`, `load()` / `save()` (schema 1;
a bare array is migrated; newer schemas are refused), `exportJSON()` / `importJSON()`, `reset()`.
The AI coach gets the text through `setProfileProvider(playerPromptText)` (exported by `main.ts`),
or `window.deadlockPlayerProfile.promptText()`.

## Validation on the reviewed recordings

`rec4/replay9.jsonl` then `rec5/replay4.jsonl` (Grey Talon), with recorded wall time:

| | rec4 | rec5 | human review |
| --- | --- | --- | --- |
| Deaths | 5 | 4 | 5 / 4 |
| Time dead | 22% | 21% | ~25% / 17-21% |
| Unspent at death (1,500+) | 4 of 5 (1.4k-3k) | 1 of 4 | every rec4 death 1.4k-3k |
| Over 3k, alive time | 26% | 44% (peak 6.3k) | 30% / ~43% |
| Killers | melee x2, Hyper Beam, Chimera | Victor's aura x2, Drifter, gun damage | rec5: every death in melee range (Victor, Drifter) |
| Enemy-dead windows unused | 2 (20:28, 32:07) | 1 (38:21) | 20:24, 32:07 / 37:24-38:38 |
| Jungle (alive) | 15% | 17% | 7-17% / ~11% (not a problem) |
| Fights joined | 8 of 11 | 11 of 16 | 9 of 14 / 7 of 12 |

Weaknesses produced: unspent souls, killed at close range (5 of 6), dying too often (21%, 2.8 per
10 min), dies soon after respawning, enemy-dead windows unused (3 of 3), gets burst down, stays in
at low health, same killer again (Victor x2, melee x2). Strength: keeps lanes covered (76% lanes,
16% jungle). No "too much jungle". The unit test `player profile on the reviewed recordings
matches the human reviews` checks this when the recordings are present (with and without wall
time).
