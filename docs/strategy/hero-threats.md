<<<<<<< HEAD
# Hero threats, crowd control and recovery (live reactions)

Retrieved: 2026-10-28

Sources:
- https://deadlock.wiki/Victor (Aura of Suffering redirects here) [WIKI]
- https://deadlock.wiki/Drifter [WIKI]
- https://deadlock.wiki/Grey_Talon [WIKI]
- https://deadlock.wiki/Health_Regen [WIKI]

Purpose: excerpts the live "active" coach quotes when it reacts to a debuff on the player, a
repeat killer, or the player leaving a fight weak. Quotes are verbatim from the pages above
(markup removed). Lines marked [INFERENCE] are coach design logic, not from a source.

## 1. Victor: Aura of Suffering
- [WIKI] Victor page intro: "Get too close and risk getting caught in his Aura of Suffering, but hey, at least you'll be suffering together."
- [WIKI] Aura of Suffering: "Unleash pain, dealing spirit damage over time to both enemies and yourself."
- [WIKI] Aura of Suffering: "The damage continues to increase the longer the ability is channeled, up to a maximum amount."
- [WIKI] Aura of Suffering radius: 8m. Its effect "can be dispelled" (Dispel Magic).
- [INFERENCE] The debuff badge on your HUD means you are inside the 8m aura now: every second you stay, it hurts more. Leaving the radius is the counter.

## 2. Drifter: Stalker's Mark, Rend, Bloodscent
- [WIKI] Drifter page intro: "He can hunt his victims with bloodscent alone, marking and rending isolated targets to pieces in an eternal night."
- [WIKI] Stalker's Mark: "While the enemy is bleeding you can re-activate Stalker's Mark to instantly appear behind their back."
- [WIKI] Stalker's Mark: "Stalker's Mark has no range limit, allowing Drifter to hit enemies across the entire map."
- [WIKI] Bloodscent: "Isolated heroes leave a trail as they move, and emit their heartbeat from afar." and "You deal amplified damage against isolated heroes" (isolation range 20m).
- [WIKI] Rend: "If the enemy is in close range, deal bonus melee damage." Its T3 "Silences enemies for 2s if close range".
- [INFERENCE] With the mark on you, Drifter can arrive behind you within its 5s duration; standing next to a teammate removes the isolation bonus.

## 3. Grey Talon: a long-range hero
- [WIKI] Grey Talon page intro: "Grey Talon is a bow-wielding mystical Hero that harasses his enemies with arrows and spirit from long range."
- [WIKI] "He uses traps and brief flight to maintain his distance"
- [INFERENCE] Deaths where melee hits or close-range auras did most of the damage mean the player let the fight come to them; the hero's strength is distance.

## 4. Recovering health
- [WIKI] Health Regen: "Standing in base will quickly regenerate the player's health (adding +60 health per second and +6% of max health per second), allowing them to recover from fights."
- [WIKI] Health Regen: out of combat regen starts "4 seconds after damaging/being damaged by heroes."
- [INFERENCE] Shopping does not heal: walking out of a shop at half health means fighting the next hero at half health.
=======
# Hero threats and counter items (death review)

Retrieved: 2026-10-28

Source:
- https://deadlock.wiki/Victor [WIKI]
- https://deadlock.wiki/Drifter [WIKI]
- https://deadlock.wiki/Spirit_Shielding [WIKI]
- data/items.json item descriptions (Dispel Magic, Reactive Barrier, Debuff Reducer) [WIKI-derived game data]

Legend: **[WIKI]** quoted from the page on the retrieval date, **[INFERENCE]** coach advice derived
from the quoted facts (not from a guide). The death review (`data/coaching/review.json`) cites these
quotes for the abilities it names from the "Incoming damage (last death)" panel and the debuff
badges it reads on the player.

---

## Victor: Aura of Suffering
- [WIKI] "Get too close and risk getting caught in his Aura of Suffering"
- [WIKI] Aura of Suffering: Radius 8m, Duration 8s; "Unleash pain, dealing spirit damage over time to both enemies and yourself."
- [WIKI] "The damage continues to increase the longer the ability is channeled, up to a maximum amount."
- [WIKI] "On T3, enemies within Aura radius take +15% Damage Amp from all sources."
- [INFERENCE] The aura is short-ranged and ramps up: a ranged hero standing inside it is fighting on
  Victor's terms. Step back out of 8 m as soon as the badge appears; spirit resist (Spirit Shielding)
  is the cheapest item answer when Victor keeps killing you.

## Drifter: Stalker's Mark, Rend, Bloodscent
- [WIKI] Drifter "can hunt his victims with bloodscent alone, marking and rending isolated targets to pieces in an eternal night."
- [WIKI] Stalker's Mark: "While the enemy is bleeding you can re-activate Stalker's Mark to instantly appear behind their back."
- [WIKI] "Stalker's Mark has no range limit, allowing Drifter to hit enemies across the entire map."
- [WIKI] Stalker's Mark: "Effect can be dispelled" (Dispel Magic); duration 5s.
- [WIKI] Rend T3: "Silences enemies for 2s if close range"
- [WIKI] Bloodscent: "You deal amplified damage against isolated heroes" (Isolation Range 20m).
- [INFERENCE] Bloodscent damage on the death panel means you were isolated (no teammate within
  20 m). When Stalker's Mark is on you, Drifter is coming to you: stand next to a teammate, leave the
  zipline, and keep your escape for the moment he lands behind you. Leaving "on the first big hit" is
  too late, because the commit starts with the mark, not the hit.

## Counter items (from data/items.json, verified by the review test)
- [WIKI] Spirit Shielding (1,600): "Gain a Barrier whenever you take significant spirit damage from enemy Heroes in a small time frame." +18% Spirit Resist.
- [WIKI] Dispel Magic (3,200): "Purge all non-ultimate negative effects currently applied to you."
- [WIKI] Reactive Barrier (1,600): "Gain a Barrier when you are Stunned, Chained, Immobilized, Slept or Silenced."
- [WIKI] Debuff Reducer (1,600): "Reduces the duration of all negative effects applied to you."
>>>>>>> wt/review2
