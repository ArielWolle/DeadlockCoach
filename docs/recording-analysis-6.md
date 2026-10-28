# Analysis 6: data audit of rec5, active coaching, AI coach and player profile

Built in parallel by independent agents on separate branches (worktrees), each validated against
the rec5 screenshots, then merged and replayed end to end.

## Data audit and fixes (rec5, 459 gameplay frames, newest pipeline)

| Field | Before | After |
| --- | --- | --- |
| Screens | 62 Unknown | Shop (alive / while dead), PostGame, Loading, Menu classes; 3 Unknown |
| Souls | 309 / 449 | 458 / 459 (held values carry `souls_age_ms`) |
| Own hero | 242 / 449 | 454 / 459 (own slot from the portrait backdrop; not a fixed slot) |
| Ally slot identities | Haze read as Vyper/Infernus | Latched votes, real top-bar templates (Haze, Danny, Shiv) |
| Your lane / mirrored-layout flips | 314 / 9 flips | 327 / 0 flips; zone label ("YORK : FACTORY") read on 410 frames |
| Team net worth after the recording gap | 0% correct, enemy missing | 12 / 12 verified frames; per-hero labels 71 / 71 |
| Top-bar dead heroes | recall 33%, nothing on death screens | 20 / 20 verified, read on death screens |
| Respawn timers | ~30% digit drops | countdown tracker, 0 inconsistent reads |
| Scoreboard K/D/A, enemy items | none | read with digit templates |
| Death panel | top row only, noisy | rows, killing blow, total; re-read while it fills in |
| Status effects | none | stunned, silenced, out of stamina, knocked off zipline, debuff badges |
| Level, unsecured souls | none | read |

Remaining gaps: two stacked portraits away from you, kill feed, ability cooldowns, Victory banner.

## Active live coaching (`crates/engine/src/active.rs`, `data/coaching/active.json`)

Debuff and crowd-control reactions (Victor's Aura of Suffering, Drifter's Stalker's Mark, stunned /
silenced / knocked off the zipline while losing health), enemy-dead push windows naming the lane,
spend souls in base, heal before going back out, unsecured souls in danger, repeat killer with a real
counter item, and missed team fights. On rec5: 7 calls in 29 minutes, none wrong.

## Death reviews (`review.rs`)

Damage is attributed to enemy heroes from the panel (ability names), with share, solo kills, number
of heroes, repeat killers by hero, crowd control before death and hero-specific lessons. rec5:

> Why you died: Drifter is the only hero named on the damage panel, with Stalker's Mark and Rend:
> likely a solo kill. Stalker's Mark was on you ... You were stunned, silenced and knocked off the
> zipline before you died. That is 2 deaths to Drifter. You died holding 3,467 souls. Next time: When
> Stalker's Mark is on you, stand next to a teammate, get off the zipline and save your escape for
> when he lands. Buy Dispel Magic (3,200) ...

## AI coach (optional) and player profile

- `docs/ai-coach.md`: any OpenAI-compatible or Anthropic gateway; text summary only, never images.
- `docs/player-profile.md`: strengths and weaknesses learned across matches, stored in the browser,
  fed to the AI prompt and spoken as one focus reminder. On rec4 + rec5 it found the reviewers'
  weaknesses (dying, unspent souls, close-range deaths, repeat killers, unused windows) and not
  "too much jungle".
