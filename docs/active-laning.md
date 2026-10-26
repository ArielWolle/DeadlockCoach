# Active, document-grounded lane coaching

The strategy documents now drive a prioritized lane policy rather than remaining only a
searchable reference. `data/coaching/laning.json` contains thirteen speech/rationale cards
with exact excerpts, document paths, sections and WIKI/COMMUNITY labels. Engine tests verify
that every excerpt still exists in its source document. Numerical thresholds are explicitly
coach heuristics, not asserted game facts.

## Decisions

Each analyzed gameplay frame can produce one current plan, independently of whether speech
is currently on cooldown:

| Plan | Required evidence / intention |
| --- | --- |
| Back off | Plausible low personal HP, a substantial observed HP drop, or being ahead of wave cover with a nearby enemy marker |
| Hold your side | More visible enemy rings near the player than allied rings including the player |
| Catch wave | Stable focused-lane front near own side plus approaching enemy trooper-dot candidates |
| Push wave | Confirmed healthy readings, player located in that lane behind allied wave cover, and a wave that still needs clearing |
| Short poke | Confirmed healthy readings, allied trooper-dot advantage, visible nearby opponent, and player behind wave cover |
| One-camp window | Own focused lane looks crashed, healthy player located there, no visible enemy ring nearby, no endangered lane/objective-preparation conflict |
| Conditional gank | Crashed own wave, healthy player, matched critical enemy in another nearby lane and located non-critical allied support |
| Positive wave position | Recognize confirmed own wave priority without asserting a kill or a safe route |
| Shop and return | A crashed own wave and sufficient unspent souls |
| Prepare rotation | Wave priority before a scheduled Urn spawn, or visible allied grouping near a lit Midboss icon |
| Secure lane income | Conservative laning fundamentals when a more specific plan lacks evidence |
| Need evidence | Personal lane/wave information is insufficient; show what is missing instead of inventing an aggressive call |

Poke and jungle calls remain conditional. The coach does not know enemy cooldowns, exact
escape resources, unseen enemies, camp availability or complete fight state. In particular,
zero visible enemy rings is not proof of safety. Jungle text says **if the camp is up** and
limits the suggestion to one nearby camp before returning to the wave. No unsupported named
camp route is asserted. Timing-sensitive orb/deny execution is not inferred from five-second
sampling; the advice concerns lane decisions and priorities.

## New screenshot evidence

- A unique small bright-green minimap ring is treated as a player candidate. Two nearby
  consecutive observations support a player-position/lane estimate; ambiguous or absent
  candidates remain unknown. Projection uses the existing calibrated lane polylines.
- Personal HP uses the dominant large numeric word in the tilted health panel and a
  separate maximum-health stat region. Font-area selection avoids taking smaller max/damage
  numbers as current HP. The nearby Spirit stat is excluded from maximum-HP OCR.
- Healthy trade/farm recommendations require multiple plausible HP reads. Low/falling HP
  can produce a conservative back-off warning earlier. Dead/spectated HUD values are excluded;
  scoreboard/menu/shop frames do not receive personal lane orders.
- Minimap history spans up to 30 seconds while rejecting gaps over 20 seconds, accommodating
  slower OCR cycles. Current positions and trooper candidates are not promoted to omniscient
  state. Lane-front estimates still require three consistent observations.

Saved-frame regressions verify 413/769 HP and 779/964 HP, Blue-lane marker estimates, missing
player markers and dead-screen suppression. These are calibration checks, not a claim that
all HUD scales, green portraits, overlays or game patches are recognized correctly.

## UI and runtime

The Coach page has a **Next best move** panel with:
- an Auto/Green/Blue/Yellow lane focus (manual focus is explicit context, not detected identity);
- current plan, rationale, personal readings and the lane source;
- separate observed evidence and still-unknown information;
- source excerpts with buttons opening the corresponding Field guide document.

**Active lane coaching** is enabled by default and can be disabled in Settings. Generic
game profiles remain observation-only and do not inherit Deadlock lane advice. Native Brain
users can set `COACH_LANE=green|blue|yellow`; replay snapshots can also carry
`coaching_context.focus_lane`.

Speech is prioritized and deduplicated. Low-health calls can interrupt the normal eight-second
gap immediately, while repeated identical calls retain their cooldown. A source frame
that is too old still has its speech discarded by the browser. Existing money/map cues are
constrained to avoid sending a player shopping/farming during a visibly uncleared personal wave.

## Timing corrections

Camp respawns depend on full clearing, so the old fixed recurring Hard camps timer was
removed. Zipline boost unlock is a one-shot reminder at the configured unlock time, using
`interval_secs: 0`; it no longer repeats on a match-clock schedule as if it observed usage.
Global Urn/Power-up reminders remain separate from tactical plans.

## Extending coverage

The first policy uses verified principles from laning, common-mistake and jungle documents.
Other cue-library entries require additional detectors (enemy HP/cooldowns, orb tracking,
structure state, cover or ability recognition) before they should become confident situational
orders. Add source-grounded cards plus meaningful decision tests as those observations become
available. Do not turn general MOBA hypotheses or UNVERIFIED facts into asserted live state.
