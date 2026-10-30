# Independent coach review: rec8 (Grey Talon, loss) replayed through the newest coach

Same rubric as [coach-review-13.md](coach-review-13.md), applied to an earlier recording to measure
progress. rec8 was coached live by an old build; here its 600 screenshots (game clock 13:25-50:26)
go through the newest vision (`replay_dir`) and the newest rule engine with the web registration
(`active_replay`, branch `feat/browser-coach` at `7da548e`). AI calls cannot be replayed; the AI
prompts and the end-of-game digest were checked with the offline evals (fake model). Tracking across
reviews: [coach-quality.md](coach-quality.md).

## Scores (new rule coach, replay)
| Source | Calls | Great | Useful | Generic | Mistimed | Wrong | Redundant |
|---|---|---|---|---|---|---|---|
| rec8 old live rule (earlier review, n=124) | 124 | 24% (great + useful) | | 58% | 8% | 6% (+3% unsupported) | |
| rec8 old live AI (earlier review) | 6 | 33% | | 33% | 17% | 17% | |
| **rec8 new rule** | 55 | 15% | 44% | 22% | 13% | 2% | 5% |
| rec13 old live rule (review-13) | 139 | 4% | 35% | 35% | 9% | 10% | 8% |
| **rec13 new rule** | 54 | 15% | 44% | 24% | 9% | 2% | 6% |

Helpful (great + useful): rec8 24% → **58%**, rec13 39% → **59%**. Generic 58% → 22% (rec8).
The fake-model AI cannot be graded; its prompts are reviewed below.

Cadence (same window, my count of the live log: 173 rule + 6 AI calls in 13:25-51:16):

| | rec8 old live | rec8 new | rec13 old live | rec13 new |
|---|---|---|---|---|
| Calls / min | 4.73 | **1.45** | 4.15 (review-13: 3.9) | **1.45** |
| Max calls in any 30 s / 60 s | 9 / 13 | **3 / 4** | 6 / 8 | **3 / 4** |
| Calls per fight-mode episode (max) | n/a | 2 (18 calls in 36 episodes) | n/a | 3 (20 in 33) |
| Death reviews per death | 1.5 (9 for 6, first death none) | **1.0** (6 for 6) | 2.2 (13 for 6) | 0.83 (5 for 6) |
| Deaths with a safety warning in the 30 s before | 4 / 6 | 4 / 6 | 4 / 6 | 6 / 6 |
| Median lead of the LAST warning before the death | 4.5 s | **2.5 s** | 7 s (review-13: ~8 s) | 5 s |
| Median lead of the FIRST warning in those 30 s | 9 s | 13.5 s | 15 s | 12.5 s |
| Words per call, median / p90 | 16 / 29 | 15 / 22 | 17 / 34 | 13 / 21 |
| Most repeated family | timer 22, wave:overview 22 | back_off 6, hold_base 6 | ai:periodic 20, map:wave 16 | back_off 9, critical 8 |

Family counts, rec8 new: lane:back_off 6, death:review 6, active:hold_base 6, pos:gank 4,
pos:enemies_down 4, lane:critical 4, plan:urn 3, and 1-2 of 16 others. No timer, wave:overview,
map:wave, unspent or last-hit readouts are left (the old live log had 22 + 22 + 13 + 10 of them).

Timing per rec8 death (new coach): 16:59 back_off 1 s before; 20:38 back_off 6 s; 27:56 urn_hold
23 s and critical 2 s; 42:00 nothing in the 30 s before (hold-base calls at 40:45/40:54, 66 s
earlier, before the first walk-out); 46:40 back_off 21 s, critical 3 s; 49:38 none (Patron defence).
The coach now warns about decisions (urn hold, hold base, outnumbered) earlier than before, but the
health calls that remain the "last warning" still land 1-3 s before the death.

Length and voice: death reviews are 21-26 words and follow "You died because ... Next life: ...
Buy/Spend ..." (the ≤ 30-word budget holds); fight-mode calls are 3-9 words. Over budget: praise
22:39 (31 words), rec13 review:pattern 31:04 (37) and praise:lead 12:12 (27). Teammates and enemies
are named (Shiv, Baba, Bebop, Pocket, Haze, Kelvin) and plans say "we". Still robotic: the same
"Back behind cover: you're getting low." six times, "Health critical: get out now." four times,
"Back out: Shiv and Baba are down." twice 20 minutes apart; a template bug ("You died because They
burst you ..."); numerals switch between "2 enemies" and "Two enemies".

## A) The match as a coach sees it
Grey Talon with Shiv, Paige, Venator, Baba and Bebop against Pocket, Rat King, Yamato, Haze, Kelvin
and Deadman Danny. The recording opens at 13:25 with the team 8k behind; by 16:30 it is +10k, +25k at
20:00, +23-33k from 22 to 37 min, and +41k at 39:22 after our Midboss (38:41). From 41:07 the lead
collapses: +12k at 41:46, +2k at 44:19, -16k at 46:37, -49k at 50:26 with our Patron (Hidden King)
under attack from 48:20. Defeat. K/D/A 9/5/13 at 47:57 (one more death after).

Deaths (verified on the screenshots and the snapshots):
1. **17:00**, Yellow: Rain of Arrows forward at 16:54 (100%), landed in a corridor on Pocket and
   Yamato, 53% at 16:58, dead by 17:00 (melee 628, Pocket 428).
2. **20:40**, deep in their Yellow base: a second Rain of Arrows in at 20:27 after a won push, slowed,
   92% → 41% in 8 s (Tankbuster, Barrage, Flying Slash, Arctic Beam: 4.3k).
3. **27:57**: carried the Urn 27:09-27:39 with Shiv and Venator dead (the coach said hold, see below),
   then walked into Blue in their half and was killed (Yamato's Flying Slash + melee).
4. (39:11, not a death) afflicted by Pocket at 19% deep in their base (0.52, 0.31): the grey
   "Respawn in: 1" screen is a **Rejuvenator revive** in place, a team credit spent alone.
5. **42:05**: 40:45 three teammates dead; he walks out to Green at 40:54, gets afflicted to 1% (41:23),
   heals in base, and walks out again at 41:44 with three teammates still dead: Kelvin's Arctic Beam.
   The lead falls 38k → 5k while he is dead; our Blue Walker falls at 43:01.
6. **46:43**, mid: afflicted in the open 46:19-46:37 with Bebop down; Affliction 2.2k + Frost Grenade.
7. **49:42**, defending our Patron: Haze's Bullet Dance (2.0k) at 100% → 0 in 8 s.

The match was lost between 39:00 and 45:00: a +41k lead with the Rejuvenator was spent on solo
trips (one Rejuvenator credit, two walk-outs with three teammates dead, a 3% walk-out at 44:24).
Pocket (their richest hero, 37k at 31:05) and his Affliction touched four of the last five
engagements. A coach's three themes: ult out, not in; with three dead, nobody leaves base; Pocket is
the threat after 35:00 (Debuff Remover / Spirit Shielding, fight out of his line).

## B) Every new rule call, graded
G = great, U = useful, Gen = generic, M = mistimed, W = wrong, R = redundant. `!` = fight mode.

| Clock | Call (key) | Grade | Note |
|---|---|---|---|
| 13:32 | 2 teammates are dead. Play safe near our guardian... (pos:allies_down) | U | 2 dead, -8k, Guardian standing |
| !13:51 | Back out: Shiv and Baba are down. (review:fight-lost) | M | 3-for-3 trade (Rat King, Yamato, Kelvin dead), you at 69% |
| !14:11 | Back behind cover: you're getting low. | Gen | 68 → 46%, kept farming safely |
| 14:57 | You're heading out at 46% health. Heal in base first; it's fast. (active:heal) | U | right; ignored until 15:54 |
| 16:34 | Midboss is up. Push your lanes first, then we group for it. | Gen | readout, no plan |
| !16:58 | Back behind cover: you're getting low. | M | 1 s before death 1 |
| 17:09 | You died because you were in their half with 3 enemies missing... Spend 2,593 souls now. | U | true; misses the forward ult and Pocket |
| !18:38 | Back behind cover... | U | 92 → 55%, survived |
| 19:27 | Urn in 33 on Yellow side, 2 of them dead: grab it before they're back. (plan:urn) | G | plan with side, numbers, timing |
| !20:32 | Back behind cover... | U | 6 s before death 2; late but actionable |
| 21:10 | You died because They burst you from 92% in 8 seconds. Next life: ... behind Bebop. | Gen | description, not a cause; "They" bug; misses the ult dive |
| 22:39 | Good use of the 21:39 window ... Also, our soul lead grew from 9,000 ... to 28,000 ... (31 w) | U | contextual, over budget |
| 23:24 | Clear the Green lane wave first. Crash it... (lane:push) | Gen | wave chore at +23k, 23 min |
| 23:42 | Fall back: 1 enemy closing through the jungle... (pos:gank) | Gen | full HP, one enemy, nothing happened |
| 24:27 | Urn in 33 on Green side: we contest it together. You hold back and Owl their carrier. | G | hero-specific job |
| !24:58 | Fall back: 1 enemy closing through the jungle. | Gen | 96%, nothing happened |
| 26:07 | Our lead dropped from 24k to 14k. No fights in their base: farm and group. (lead:falling) | U | true; "farm and group" vague |
| 27:09 | 2 enemies are dead ... We push a lane or take an objective now. | Gen | you are picking up the Urn |
| 27:33 | Hold the urn: two teammates are dead and an enemy is close. Back off and wait for them. | G | exactly right, 23 s before death 3 |
| !27:54 | Health critical: get out now. | M | 2 s before death 3 |
| 28:02 | You died because you were in their half with 4 enemies missing... | Gen | 3 of 5 alive were visible; misses the two dead teammates it had flagged |
| 28:50 | 2 of your deaths came up close. ... fight from max range ... (active:close) | R | second death-screen message |
| 29:41 | Three enemies are dead for 17 more seconds. Group up and push Yellow lane now. | M | you are in Blue; Yellow is too far for 17 s |
| 31:11 | Two enemies are dead for 19 more seconds. Group up and push Yellow lane now. | M | same; you pushed Blue (lead +15k → +26k) |
| !32:05 | Fall back: 1 enemy closing through the jungle. | U | deep in their base at 78%, slowed |
| 32:52 | 2 enemies are dead, the first back in about 21 seconds. We push a lane or take an objective now. | Gen | 3 were dead; no target |
| !33:40 | Back out: Shiv and Baba are down. | Gen | 2-for-2 at +33k, you at 98%; same text as 13:51 |
| 34:25 | Urn in 35 on Green side: we contest it together... Start walking now. | U | good, repeated template |
| !34:52 | Don't go in alone: we're losing at Green lane. (active:team_losing) | U | stops a solo Urn walk into a lost fight |
| 35:46 | 3 teammates are dead, the first back in about 19 seconds. Play safe near our guardian... | U | |
| 35:55 | Hold in base: three teammates are dead, the next back in 10 seconds... | R | same message 9 s later |
| 36:39 | Back off, you're outnumbered: 2 enemies on just you. Wait for help. | G | 98% → 56% in the next 10 s; survived |
| 37:38 | We're 28k up but Pocket is their whole game. No fights in their base without Rejuvenator. | G | names the fed enemy and the throw risk |
| 38:41 | Good use of the 37:32 window: with three enemies dead, you and your team pushed Blue lane up. | U | contextual, but we just took Midboss: the plan was the call |
| !39:07 | Back behind cover... | U | 73 → 52%, afflicted deep; ignored |
| 40:42 | The urn is at the Green side bridge. Check who is missing before you grab it. | W | three teammates dead: invites a solo walk; contradicts the next call |
| 40:45 | Hold in base: three teammates are dead, the next back in 23 seconds. Don't walk out alone. | G | the key call of the match |
| 40:54 | Go back to base: three teammates are dead, the next back in 14 seconds... | G | correct escalation; ignored |
| !41:23 | Health critical: get out now. | U | 1%, afflicted; he escaped |
| 42:12 | You died because Kelvin and Yamato focused you. Next life: ... leave on the first big hit. | Gen | misses the decision: second walk-out with 3 dead |
| 43:01 | Our lead dropped from 39k to 6k. No fights in their base: farm and group. | M | drop began 41:07; we are defending, not in their base |
| 43:49 | Haze is missing: Bullet Dance shreds solitary targets. Stay with your team, Snare her landing. | U | named threat + counter |
| !44:02 | Health critical: get out now. | U | 63 → 23 → 1%, survived |
| !44:24 | You're heading out at 3% health. | G | walking mid-map at 3-4% |
| 44:34 | Fall back: 1 enemy closing from Blue lane... | U | at 6% HP |
| !44:53 | Hold in base. | U | four teammates dead |
| 45:28 | Go back to base: three teammates are dead... | U | four were dead |
| !46:19 | Back behind cover... | U | 21 s before death 5, afflicted in the open |
| !46:37 | Health critical: get out now. | M | 3 s before death 5 |
| 46:51 | You died because Bebop went down first and you stayed in... Buy Spirit Shielding now. | U | true; Pocket's Affliction not named |
| 47:38 | Pocket's Affliction topped 2 of your deaths. Track him before you commit. (active:repeat) | U | right threat, but a second death-screen message |
| 48:11 | Wait for them: two teammates respawn within 5 seconds. Walk out together. | U | |
| 48:20 | Go defend our Patron: it's under attack. | U | Hidden King at 50% |
| !49:34 | Go defend our Patron: it's under attack. | R | already defending; same text |
| 50:14 | You died because Haze and Rat King focused you. Next life: fight behind Bebop... | Gen | Bullet Dance not named; no counter (Snare her, Metal Skin) |

Per family (rec8 new): plans (plan:urn, lead:fed) 3 G / 1 U; active:* (hold base, heal, urn hold,
base attack) 4 G / 7 U / 3 R, the strongest family; outnumbered 1 G; lead:falling 1 U / 1 M; death
reviews 2 U / 4 Gen; health calls (back_off, critical) 6 U / 1 Gen / 3 M; pos:enemies_down 2 M / 2 Gen
(lane choice and counts); review:fight-lost 1 M / 1 Gen; pos:gank 2 U / 2 Gen; praise 2 U.

Best 5:
1. 40:45 + 40:54 "Hold in base: three teammates are dead, the next back in 23 seconds. Don't walk out alone." / "Go back to base..."
2. 27:33 "Hold the urn: two teammates are dead and an enemy is close. Back off and wait for them."
3. 37:38 "We're 28k up but Pocket is their whole game. No fights in their base without Rejuvenator."
4. 24:27 "Urn in 33 on Green side: we contest it together. You hold back and Owl their carrier."
5. 44:24 "You're heading out at 3% health." (and 36:39 "you're outnumbered: 2 enemies on just you")

Worst 10:
1. 40:42 urn-ground: "Check who is missing before you grab it" with three teammates dead (wrong).
2. 42:12 death review blames "Kelvin and Yamato focused you": the cause was a second walk-out with
   three teammates dead, after two hold calls.
3. 43:01 lead:falling two minutes late, "No fights in their base" while the enemy is in ours.
4. 21:10 "They burst you from 92% in 8 seconds": a symptom, a capitalisation bug, no ult lesson.
5. 29:41 / 31:11 "push Yellow lane now" with 17-19 s left while you stand in Blue.
6. 13:51 / 33:40 "Back out: Shiv and Baba are down" on even trades (3-for-3, 2-for-2 at +33k).
7. 16:58 / 27:54 / 46:37 last health calls 1-3 s before the death.
8. 28:02 "4 enemies missing" (3 of 5 alive were on the minimap) instead of "two of ours were dead".
9. 28:50 / 47:38 second messages on the death screen (one death = one message).
10. 32:52 "2 enemies are dead" (3 were) and "push a lane or take an objective": no target.

## C) The 10 most important moments
| Clock | A high-level coach says | The new coach says |
|---|---|---|
| 16:54 (ult forward in Yellow, Pocket + Yamato unseen) | "Don't ult in, three are missing. Rain of Arrows is your way out, not in." | 16:58 "Back behind cover" (1 s before death); review "in their half with 3 missing" |
| 20:15 (deep in their Yellow base, Pocket and Rat King back soon) | "Rat King and Pocket are back in ten: leave their base now, we take the Urn on our side." | 19:27 good Urn plan; then 20:32 "Back behind cover"; review "They burst you" |
| 22:04 (4 enemies dead, +24k) | "Four of theirs dead for 20 seconds: their Green Walker now with Shiv and Baba." | silence; praise for the window at 22:39 |
| 27:39 (Urn delivered, Shiv + Venator dead) | "Urn's in. Two of ours are down: back to our side until they're up." | 27:33 "Hold the urn ... wait for them" (great); 27:54 critical; review misses it |
| 37:38 (+28k, Pocket richest) | "We're 28k up, Pocket is their whole game: no base dives without Rejuvenator." | same call (great) |
| 38:41 (we took Midboss, +36k) | "We have Rejuvenator for three minutes: all five to their Shrine now." | praise for the 37:32 window; no plan |
| 39:07 (afflicted at 52%, deep, alone) | "Pocket's Affliction on you: out now, don't spend our Rejuvenator alone." | 39:07 "Back behind cover"; the revive at 39:17 goes unremarked |
| 40:45-41:44 (three teammates dead, two walk-outs) | 40:45 "Hold, three are down." 41:44 "Still three down. Stay: Kelvin and Pocket are at Green." | 40:45 + 40:54 hold (great); nothing on the second walk-out; 40:42 Urn invite |
| 41:30 (lead 38k → 24k in 25 s) | "We're throwing: everyone back to our Walker, nobody fights alone." | 43:01 "Our lead dropped from 39k to 6k. No fights in their base" |
| 46:19 (afflicted mid, Bebop down) | "Bebop's down and you're afflicted: back to our Shrine now. Debuff Remover next." | 46:19 "Back behind cover"; review "Bebop went down first ... Spirit Shielding" (useful) |

## D) Remaining gaps → prioritised improvements
1. **Re-arm "hold in base" on every walk-out and make ignored calls a death cause.** rec8 41:44 second
   walk-out with three dead: no call, death 42:05; the review blamed "Kelvin and Yamato focused you".
   Death review priority: "walked out with 3+ teammates dead" and "ignored <hold/urn_hold/outnumbered>
   call in the last 60 s" before "X and Y focused you" (rec8 28:02, 42:12; rec13 41:20).
2. **Rejuvenator state.** Own Midboss → plan "We have Rejuvenator for three minutes: all five to
   <standing structure>" (rec8 38:41 and rec13 30:13 had none; rec13 32:18 said "without
   Rejuvenator" while we held it). Detect the grey "Respawn in: 1" revive (rec8 39:17) → "That was our
   Rejuvenator credit: don't spend it alone"; pass it to the digest.
3. **Throw detector reacts in seconds.** Trigger on ≥ 10k lost in 60 s or ≥ 25% of the peak, refresh
   the numbers, fit the text to where the fight is ("defend our Walker together"). rec8 fired at 43:01
   for a drop that began 41:07; rec13 42:08 said "46k to 35k" when it was 10k.
4. **Forward-position warnings with enemy respawn timers** (the last warning is 2.5 s before death in
   rec8). When you are in their half and the window closes in ≤ 10 s or 3+ are unseen: "Pocket and Rat
   King back in 10: leave their base." rec8 20:15-20:38, 27:39-27:57, 39:00-39:11; rec13 40:33-41:04.
5. **Ultimate context for Grey Talon.** The ability bar is read every 2 s: Rain of Arrows used toward
   their half with 3+ unseen → "Ult's down and three are missing: back to Bebop." Deaths 1 and 2 of
   rec8 followed a forward ult (16:54, 20:27, screenshots).
6. **Death reviews name the killer's ability and the counter.** rec8: 4 of 6 reviews generic ("They
   burst you", "X and Y focused you"); fix the "They" template; name Affliction → Debuff Remover /
   Spirit Shielding, Bullet Dance → Snare her, Metal Skin; fold active:close / active:repeat into the
   one review (28:50, 47:38).
7. **Enemy-dead windows pick a reachable target.** Use the player's lane or the nearest standing
   structure (structures are now read) and drop windows under ~20 s from far away: rec8 29:41 / 31:11
   "push Yellow" from Blue with 17-19 s; 22:04 four dead with no plan; counts wrong at 32:52 (2 vs 3).
8. **Fight-lost only when we lost the trade.** Require allies dead > enemies dead in the fight and
   vary the text: rec8 13:51 (3-for-3), 33:40 (2-for-2 at +33k), identical wording.
9. **Contradiction guard.** No objective pickups or pushes while ≥ 3 teammates are dead or a hold call
   is live: rec8 40:42 Urn invite 3 s before "Hold in base"; rec8 27:09 "push a lane" while carrying
   the Urn; rec13 26:00 Midboss plan while carrying the Urn.
10. **Health and repeat wording.** 10 identical health calls in rec8, 17 in rec13, "critical" at 48%
    (rec13 37:53); four identical Abrams threat calls in rec13 (10:35, 14:36, 21:10, 25:12); garbled
    praise ("spent 3,200 souls 5 seconds after reaching 6,400"). Say the situation ("afflicted: get out
    of Pocket's line", "you're in their half at 55%: walk back to Bebop"); repeat a threat only on a new
    trigger; keep praise ≤ 20 words (22:39 was 31).
11. **AI prompt fixes** (offline eval, below): no "Praise: allowed" in a death prompt for the play
    that caused the death (rec8 21:01 praised "pushed Yellow lane deep at 20:14", the push that killed
    him); wait for the damage panel before the death prompt (21:01 "cause unknown", Tankbuster read at
    21:10); add Rejuvenator holder/time left; stop truncating the profile mid-word ("10:12 deat").
12. **Vision/digest facts.** rec13's clock jumps 7:48 → 6:51 at frame 17 (the first ~17 frames read
    ~57 s late; the 6:0x death got no review); digest marks rec8 deaths 2 and 4 "alone no / forward no"
    though the screenshots show a solo dive into their base and a walk-out with three teammates dead;
    add "teammates dead at death" and "ignored call" to each death, and the Rejuvenator revive.

## AI coach (offline evals, fake model)
`scripts/ai-coach-eval.ts` on the new rec8 snapshots: 49 requests (0.89/min), 16 spoken, 33 gated
(28 near-duplicates of the fake model's stock lines, 5 as a rule theme in the last 30 s), no AI call
during fights, one AI death request per death (the Rejuvenator revive did not trigger one). Spoken
texts are the fake model's and are not graded.

Prompt quality (sample periodic 21:52 and death 21:01):
- Size: periodic 3,123 + 3,701 = 6.8k chars (rec13: 9.7k live → 7.1k; death 9.7k → 7.9k); the
  generic wiki block is gone.
- Present and correct: K/D/A 4/2/7, ultimate on cooldown, "just WON a fight: Haze, Pocket", enemy-dead
  window after the ~6 s latency, lead trend (+8k → +27k), word budget, blocked themes, structures read
  per lane in the death prompt, damage by hero (Pocket 428 at 17:00), the rule review already spoken.
- Wrong or weak: praise allowed for the fatal push (above); "cause unknown" because the death prompt
  goes out before the panel; "Earlier Death at 17:00 ... respawn 3s" (stale timer); the profile is cut
  mid-word; KNOWLEDGE gives correlational matchup win rates that do not change a decision; no
  Rejuvenator state.
- Live rec8 (old build) for reference: 6 AI calls, 5 of them "spend your souls".

End-of-game digest (new snapshots + new rule reviews, built with `buildDigest`): 7.0k chars, prompt
11.6k chars. It names the throw ("+41k at 39:22 → -49k, it started to fall by 41:07"), our Midboss at
38:41, the structure losses (our Blue Walker 43:01, Shrine 47:38), the Patron under attack and the
Defeat. Gaps: no Rejuvenator revive, "alone no" for death 4, K/D/A read before the last death
(9/5/13), and "K/D/A ... handing the enemy few kill bounties" listed as a strength in a 6-death loss.
