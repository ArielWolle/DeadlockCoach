# Independent coach review: rec13 (Grey Talon, loss, ~23/6/11) — baseline for coaching quality

Graded by an independent "high-level coach" reviewer from screenshots, the live cue log and the AI log.
Later reviews use the same rubric so coaching quality can be tracked (see docs/coach-quality.md).

## Scores (live)
| Source | Calls | Great | Useful | Generic | Mistimed | Wrong | Redundant |
|---|---|---|---|---|---|---|---|
| Rule | 139 | 4% | 35% | 35% | 9% | 10% | 8% |
| AI | 29 | 14% | 45% | 14% | 10% | 10% | 7% |

Cadence 3.9 calls/min (rule 3.2, AI 0.67); up to 8 calls in 60 s during the deciding fight; 13 of 29 AI
calls repeated a rule call within 30 s; median warning lead time before a death ~8 s; AI latency 5.8 s.

## Worst failures
- One death produced 5 death reviews (2 rule, 3 AI), two with false facts ("died 0 / 5 s after respawning")
  from Unknown/Shop flicker frames during a long death.
- Death reviews miss the real cause: 29:25 "5 enemies dead, you at 15% HP hitting their Walker"; 90 words.
- Timers and wave overviews announce state without a plan (15 timer + 11 overview calls).
- `map:wave` at High priority during late-game sieges ("catch that wave before taking jungle camps");
  `lane:last_hit` after laning (9:35-14:01).
- Up to 8 calls per fight; reactive health calls instead of a plan; robotic wording ("Health looks low
  or falling"), no names, no "we".
- AI sends the team to structures that may be gone (no structure state), arrives ~6 s late for short
  windows, never names the fed enemy (Infernus) or a throw (+46k lead lost).
- Praise mistimed: "Keep it up: last game you went 9 minutes without dying" 5 s after a death.

## Prioritised changes (from the review)
1. One death = one event and one review (no flicker deaths), rule + AI merged into one voice.
2. Post-death debrief ≤ 30 words: cause (named, situation) → next life → buy.
3. Objective/fight plans 30-45 s ahead instead of timer readouts (Urn, Walker weak 18:00, Midboss, Rift).
4. One call per fight: fight mode blocks wave/timer/urn/lead calls; threat or exit trigger only, ≤ 10 words.
5. Game state: structures per lane, Midboss/Rejuvenator holder, fed enemy (net worth, streak), enemy items.
6. Throw detector: lead ≥ 15k falling or enemy Rejuvenator → protect the lead, no base dives.
7. Low HP after a won fight: heal first, objective second (said 10-20 s early, not at 1 HP).
8. Named threat calls: enemy + ability + Grey Talon counter-play (Snare on the dive path, Owl from cover).
9. Phase / priority fixes: last-hit only before 8:00; wave calls Low, never during sieges or grouping.
10. Contextual praise only (this match, last 60 s, never within 60 s of a death or contradicting mistake).
11. Itemization: short, threat-based, at shop moments (≤ 15 words); enemy items in the AI prompt.
12. AI prompt: structures, enemy items, K/D/A, fight state, lead trend, own ultimate state, match focus goal;
    drop the generic wiki block; drop AI calls duplicating a rule call within 30 s; latency-aware windows.
13. Match focus goal from the last reviews (e.g. "no deaths below 35% after a won fight"), said at start,
    on related events, scored at the end.
14. One coach voice: names, "we", reason, next action. Budget per moment type:

| Moment | Calls | Length |
|---|---|---|
| Laning 0-8:00 | 1 / 60-90 s | ≤ 15 words |
| Mid 8-20 | 1 / 45-60 s | ≤ 20 words |
| Late 20+ | 1 / 45 s outside fights | ≤ 20 words |
| Teamfight | 1 (max 2) per fight | ≤ 10 words |
| Death screen | 1 within 10 s | ≤ 30 words |
| Respawn | 1 | ≤ 15 words |
| End of game | 1 review | ~200 words |

## Target-quality moments (what a high-level coach says)
- 5:30 (30% HP vs Geist): "You're at 30 and Geist has her bomb: walk to the Guardian now, the wave can wait."
- 22:50 (zipline into Green, 3 unseen): "Hop off the zipline before the bridge: Graves is missing and his hands knock you off. Walk in behind Billy."
- 29:05 (5 dead, you at 15%): "Fight's won, don't touch that Walker at 15%. Back off, heal, and we go Rejuvenator together."
- 33:06 (6 dead): "Six down, about 40 seconds: that's Shrines, not a Walker. Go with all five."
- 36:30 (+46k, Infernus streaking): "We're 45k up but Infernus is their whole game. No fights in their base without Rejuvenator."
- 40:35 (Abrams, allies falling): "Abrams has his charge: Snare at your feet and stand behind Bebop. Bebop and Rem are down, back out now."
