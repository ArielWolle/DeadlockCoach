# Coaching quality tracking

One row per graded recording and coach version, with the rubric of
[coach-review-13.md](coach-review-13.md): every spoken rule call is graded great / useful / generic /
mistimed / wrong / redundant by an independent "high-level coach" reviewer against the screenshots
and snapshots. "old live" = what the coach said live that day; "new" = the same screenshots replayed
through the newest vision + rule engine (AI calls cannot be replayed; the AI is tracked by prompt).

## Rule coach
| Recording · coach | Build | Calls | Helpful (great + useful) | Great | Generic | Wrong | Calls / min | Max calls in any 30 s (max per fight) | Death reviews per death | Median lead of last warning before a death (deaths warned) | Words median / p90 | Review |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| rec8 · old live | live, before b873284 | 124 graded (173 spoken in 13:25-51:16) | 24% | n/a | 58% | 6% (+3% unsupported) | 4.73 | 9 (n/a) | 1.5 (9 for 6; first death none) | 4.5 s (4 / 6) | 16 / 29 | earlier review (not in docs) |
| rec13 · old live | live, before b873284 | 139 | 39% | 4% | 35% | 10% | 3.9 (my count 4.15 incl. AI) | 6; 8 in 60 s (n/a) | 2.2 (13 for 6; one death got 5) | 7 s (4 / 6); review-13: ~8 s | 17 / 34 | [coach-review-13.md](coach-review-13.md) |
| rec8 · new | 7da548e | 55 | **58%** | 15% | **22%** | **2%** | **1.45** | **3 (2)** | **1.0** (6 for 6) | 2.5 s (4 / 6) | 15 / 22 | [coach-review-8.md](coach-review-8.md) |
| rec13 · new | 7da548e | 54 | **59%** | 15% | **24%** | **2%** | **1.45** | **3 (3)** | 0.83 (5 for 6; first death none) | 5 s (6 / 6) | 13 / 21 | this file, below |
| rec8 · new | 00f02b9 (review-8 batch) | 54 | not regraded | n/a | n/a | n/a | **1.43** | 3 (2) | 1.0 (6 for 6) | 4 s (5 / 7)* | 15 / 24 | this file, below |
| rec13 · new | 00f02b9 (review-8 batch) | 57 | not regraded | n/a | n/a | n/a | **1.49** | 3 (2) | **1.0** (6 for 6) | 6 s (6 / 6)* | 12 / 21 | this file, below |

\* Measured with the automatic death list and warning-key set below (`metrics.js`, deaths = first
`snapshot.dead` frame, revives excluded; warning keys now include `forward_exit`, `escape_in`,
`ult_out`). On that same method 7da548e scores rec8 0 s (6 / 7) and rec13 0 s (5 / 6), so the
change is +4 s / +6 s. The automatic list counts rec8 28:50 as a death (no review there), hence 7.

Also tracked: mistimed rec8 8% → 13%, rec13 9% → 9%; redundant rec13 8% → 6%, rec8 new 5%. The
first warning inside the last 30 s before a death: rec8 9 s → 13.5 s, rec13 15 s → 12.5 s.

## AI coach
| Recording · coach | AI calls (live) | AI helpful | AI wrong | Periodic prompt (chars) | Death prompt (chars) | AI calls repeating a rule call ≤ 30 s | End-of-game digest / prompt (chars) |
|---|---|---|---|---|---|---|---|
| rec8 · old live | 6 (5 "spend souls") | 33% | 17% | n/a (no AI log) | n/a | 0 of 6 | n/a |
| rec13 · old live | 29 | 59% | 10% | 9,722 avg | 9,674 avg | 13 of 29 (review-13) | n/a |
| rec8 · new (offline, fake model) | not replayable | n/a | n/a | 6,824 (sample 21:52) | 6 requests, 1 per death | 5 answers gated as a rule theme | 7,003 / 11,628 (scratch build) |
| rec13 · new (offline, fake model) | not replayable | n/a | n/a | 7,057 avg (36) | 7,928 avg (6) | 2 answers gated | 8,822 / n/a |

## Trend in one line
Between the live builds and 7da548e the rule coach went from ~4-5 calls/min (9 in 30 s) to 1.45
calls/min (3 in 30 s), helpful calls from 24-39% to 58-59%, wrong calls from 6-10% to 2%, and one
death now gives one review. Not improved: the last health warning still lands 2-5 s before a death,
and death reviews pick a symptom ("X and Y focused you") over the decision in 4 of 6 rec8 deaths.
Open items: [coach-review-8.md](coach-review-8.md) section D.

00f02b9 (section D batch) keeps the cadence (1.43-1.49 calls/min, ≤ 2 calls per fight) and moves
the median last warning from 0 s to 4-6 s before a death. Death reviews now name the decision in 4 of
6 rec8 deaths ("used Rain of Arrows to go in with four of them missing", "walked out with three
teammates dead after the call to hold", "stayed deep in their half after the call to get out") and
rec13's first death has its review again (clock fix). Not regraded by a reviewer yet. Still open:
rec8 22:04 (four enemies dead) gets no plan, the throw call also fires on mid-game 10k swings, and
the 0 s leads are health calls at the moment of death.

## rec13 · new: grading notes
54 calls: 8 great, 24 useful, 13 generic, 5 mistimed, 1 wrong, 3 redundant. Great: 11:23 Guardian
plan, 17:24 Walker plan, 22:39 "Graves is missing ... not on ziplines" (29 s before the Graves death
at 23:08), 24:26 Urn plan, 29:17 "Fight's won, but you're at 33%. Back off and heal", 29:33 Walker
death review, 33:55 reset, 39:50 "They took Midboss ... defend together". Wrong: 31:33 "two enemies
are inside our base" while the team pushed Yellow at +20k (no banner read). Redundant: Abrams threat
repeated at 21:10 and 25:12 with the same text, lead:midboss repeated at 40:38. Gaps: no review for
the first death (clock misread 7:48 → 6:51 at frame 17), 41:20 review blames Bebop instead of the
solo walk into their Green base against their Rejuvenator, 42:08 lead:falling with stale numbers.

## How to re-run (add a recording)
Repo root, one cargo command at a time (`$env:CARGO_BUILD_JOBS=4;`), scratch outside the repo:
1. Vision replay of the screenshots (~8 min for 600 frames):
   `$env:CARGO_BUILD_JOBS=4; cargo run --release -q -p coach-vision --example replay_dir -- frames/<rec>/img <scratch>\<rec>-new.jsonl`
2. Rule engine with the web registration (timings minus planned ones, default rules, counters):
   `$env:CARGO_BUILD_JOBS=4; cargo run --release -q -p coach-engine --example active_replay -- <scratch>\<rec>-new.jsonl --all > <scratch>\<rec>-cues.txt`
   It prints every call (`!` = fight mode) and the cadence lines: calls/min, max in 30 s / 60 s,
   death reviews, fight-mode episodes with their call counts.
3. Old live calls: `spoken[]` of `frames/<rec>/recording.json` (fields `clock`, `source`, `key`,
   `text`); count them over the same game-clock window as the screenshots.
4. Deaths: the first frame with `snapshot.dead` per death, merging frames until the respawn timer
   ends; a grey "Respawn in: 1" screen that turns into gameplay within ~10 s is a Rejuvenator revive,
   not a death (rec8 39:17). Use the clock between the last alive frame and the first death frame.
5. Warning lead: for each death, safety calls (keys matching back_off, critical, gank, outnumbered,
   threat:, hold_base, team_losing, fight-lost, urn_hold, allies_down, missing, overextended, reset:,
   active:heal / close / cc) in the 30 s before it; report the median of the last one's lead and how
   many deaths had any.
6. Grade every new call against the screenshots and a per-frame timeline of the new snapshots
   (clock, screen, HP %, souls, team souls difference, dead allies / enemies, minimap position and
   lane, damage-panel cause, banners, `midboss_taken_by`). View at most ~12 contact sheets.
7. AI prompts (fake model, no network): in `web/`,
   `node --experimental-strip-types scripts/ai-coach-eval.ts <recording.json | replay.jsonl>`
   (prints requests, gating, prompt sizes and two sample prompts); end-of-game digest:
   `node --experimental-strip-types scripts/ai-review-eval.ts [--all] [--digest rec13]` (built-in for
   rec11-rec13; other recordings need `buildDigest` from `web/src/ai-review.ts` fed with the new
   snapshots and the new death reviews).
8. Add a row to both tables and a `docs/coach-review-<n>.md` with sections A-D.
