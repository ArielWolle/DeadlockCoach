# Hero threats, crowd control, recovery and counter items

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

---

# Part 2: hero threats and counter items (death review)

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

---

# Part 3: damage types of recorded killers (counter items)

Retrieved: 2026-10-28

Sources:
- https://deadlock.wiki/Pocket [WIKI]
- https://deadlock.wiki/Haze [WIKI]

Purpose: the counter-item suggestion names the item for the damage that topped your recent death
panels (rec8: Pocket's Affliction, Haze's Bullet Dance). Quotes are verbatim (markup removed).

## Pocket: Affliction
- [WIKI] Affliction: "Applies damage over time to enemies nearby." (in the ability text the damage is marked as Spirit Damage)
- [WIKI] Affliction: "Affliction's damage is non-lethal and does not apply item procs."
- [WIKI] Affliction: "Debuff duration reduced by Debuff Resist"
- [INFERENCE] Spirit damage over time: spirit resist (Spirit Shielding) answers it. The page does
  not mark Affliction as dispellable, so Dispel Magic is not suggested against it.

## Haze: Bullet Dance
- [WIKI] Bullet Dance: "Enter a flurry, firing your weapon at nearby enemies with perfect accuracy and added Bullet Damage."
- [WIKI] Haze "eliminates solitary enemies swiftly."
- [INFERENCE] Bullet Dance is gun damage: bullet resist items (Weapon Shielding, Bullet Resilience,
  Metal Skin) answer it.

---

# Part 4: named threat calls and Grey Talon counter-play (live plans)

Retrieved: 2026-10-29

Sources:
- https://deadlock.wiki/Abrams (Shoulder Charge redirects here) [WIKI]
- https://deadlock.wiki/Graves (Grasping Hands redirects here) [WIKI]
- https://deadlock.wiki/Lady_Geist (Essence Bomb redirects here) [WIKI]
- https://deadlock.wiki/Infernus [WIKI]
- https://deadlock.wiki/Kelvin [WIKI]
- https://deadlock.wiki/Grey_Talon (Spirit Snare redirects here) [WIKI]
- https://deadlock.wiki/Midboss [WIKI]

Purpose: the live threat call (`crates/engine/src/plans.rs`, `data/coaching/plans.json`) names the
enemy hero that is close to you or missing near your lane, the ability that makes it dangerous, and
what your hero does about it. Quotes are verbatim from the pages above (markup removed). rec13 (a
Grey Talon loss) died to Lady Geist's Essence Bomb (7:07), to Graves' Grasping Hands off a zipline
(23:09), to Infernus' Afterburn (29:25, 43:18) and after an Abrams charge (40:38).

## Abrams: Shoulder Charge
- [WIKI] Abrams page intro: "With a sudden Shoulder Charge or area stun from his Seismic Impact, he can aggressively initiate fights and isolate fragile targets from the rest of their team to set up a devastating burst of damage."
- [WIKI] Shoulder Charge: "Charge forward, pulling enemies you hit. Pushing a hero into a wall applies stun."
- [WIKI] Shoulder Charge note: "At base duration, Shoulder Charge travels approximately 17.5m."
- [WIKI] Shoulder Charge item interactions: "Triggers Reactive Barrier" (counter need "cc").
- [INFERENCE] The charge needs a straight path and a wall behind you: a trap on the path and open
  ground behind you (not a wall) take the stun away.

## Graves: Grasping Hands
- [WIKI] Grasping Hands: "Unearth a line of grasping hands, summoning a Ghoul and leaving behind a rift. Enemies who pass through take spirit damage and receive immobilize."
- [WIKI] Grasping Hands cast range 24m, immobilize 1s (T3 +0.75s).
- [WIKI] Grasping Hands item interactions: "Triggers Reactive Barrier" (counter need "cc").
- [INFERENCE] An immobilize on a zipline or in a narrow approach holds you in place for the rest of
  her team: walk in behind a teammate when she is not on the map.

## Lady Geist: Essence Bomb
- [WIKI] Essence Bomb: "Sacrifice some of your health to launch a bomb that deals damage after a brief arm time."
- [WIKI] Essence Bomb radius 7m, arming duration 0.65s; 90 Spirit Damage (counter need "spirit").
- [INFERENCE] The arm time is the window: keep moving when the bomb lands, and do not trade at low
  health inside her range.

## Infernus: Afterburn
- [WIKI] Infernus page intro: "Infernus can chase down and ignite his foes with a deadly Afterburn."
- [WIKI] Afterburn: "Weapon hits build up a burning effect, dealing spirit damage over time." (counter need "spirit")
- [INFERENCE] Afterburn builds per hit: every second spent in his gun range adds burn. Distance and
  breaking line of sight end the build-up.

## Kelvin: Frost Grenade
- [WIKI] Frost Grenade: "Throw a grenade that explodes in a cloud of freezing ice that heals allies and applies spirit damage and move speed reduction to enemies." (counter need "spirit")
- [INFERENCE] The slow sets up the rest of his team: keep stamina for the escape after the grenade.

## Grey Talon counter-play
- [WIKI] Spirit Snare: "Once armed, the trap will trigger when an enemy enters its radius, applying Curse and Movement Slow that interrupts, Silences, Disarms, and prevents item usage."
- [WIKI] Spirit Snare tip: "Placing a snare in narrow areas where enemies are likely to ambush from (or escape to) can force them to either be hit by the snare or take a less ideal path."
- [WIKI] Guided Owl tip: "Guided Owl can be used as a targeted, long-range stun that is capable of interrupting hard-to-reach channeled ults from behind cover"
- [INFERENCE] Against a diver the snare goes on the dive path (at your feet or on the approach), the
  owl is cast from cover, and the bow is used from max range behind the front line.

## Midboss and Rejuvenator (for the lead / throw calls)
- [WIKI] Midboss page: "While the Midboss is alive, a golden Rejuvenator symbol will appear at its location in the minimap."
- [INFERENCE] A team holding Rejuvenator credits revives in place for 3 minutes: fighting them in
  their base during that time trades your lives for nothing.
