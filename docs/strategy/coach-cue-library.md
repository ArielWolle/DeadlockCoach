# Coach Cue Library

Retrieved: 2026-10-25

Source (facts underlying triggers):
- https://deadlock.wiki/Strategy , /Souls , /Trooper , /Cursed_Apple , /Soul_Urn , /Unstable_Rift , /Midboss , /Rejuvenator , /Death , /Extra_Slots , /Backdoor_Protection , /Patron , /Melee_Attack
- Community: https://metabot.gg/en/deadlock/guides/deadlock-laning-phase-strategy-guide , https://metabot.gg/en/deadlock/guides/deadlock-team-fighting-positioning-guide , https://metabot.gg/en/deadlock/guides/common-deadlock-beginner-mistakes , https://games.gg/deadlock/guides/deadlock-laning-guide/ , https://www.deadlockatlas.com/en/guides/deadlock-teamfight-guide , https://www.deadlockatlas.com/en/guides/deadlock-map-objectives-guide

## How to read this
- Cue text is short (about 3-12 words) so it can be spoken in about 2-3 seconds.
- Triggers use HUD-observable data only: game clock, souls/unspent souls, item list/slots, minimap, health/stamina, top-bar timers, kill feed, scoreboard.
- Priority: **P1** = safety / immediate (speak now, interrupt others), **P2** = tactical (speak when no P1), **P3** = advisory (speak in quiet moments, rate-limited).
- Basis column: W = wiki fact behind the trigger timing, C = community advice, I = coach inference. Rows with UNVERIFIED thresholds need tuning in playtests; thresholds like "30%" or "2 min" are the coach's own choices, not game facts, unless the Basis says W.
- Suggested global rate limit: one cue per ~8s; same cue ID not repeated within 60s (I).

## Cue table

| ID | Cue (spoken) | Trigger (HUD-observable) | Basis | Pri |
|---|---|---|---|---|
| L01 | "Shoot the orange orbs. Deny them." | Clock < 10:00, enemy-colored (orange) floating orb(s) on screen, none shot over 2 consecutive frames | W floating orbs deniable (Souls); C deny value | P2 |
| L02 | "Confirm your orbs before they pop." | Green floating orb(s) near player while player not firing; orbs expire ~1s | W ally orbs auto-pop ~1s | P2 |
| L03 | "Use melee for the last hit." | Trooper at low health within melee range, clock < 15:00 | W melee last hit gives 100% instantly, no orb | P3 |
| L04 | "Push the wave in before you leave." | Player marker leaving lane toward another lane/jungle, troopers mid-lane (even) on minimap | C games.gg/metabot crash-before-leave | P2 |
| L05 | "Lane is pushed on you. Hold near your guardian." | Troopers cluster near own Guardian on minimap, clock < 8:00 | W Strategy (stay in guardian safe zones) | P2 |
| L06 | "Health low. Reset and shop." | Health < 35% and no enemy in view, unspent souls > 0 | C (reset), W Glossary "reset" | P1 |
| L07 | "You're overextended. Back up." | Player ahead of own troopers past mid-lane, clock < 8:00, enemy duo visible | W Strategy (troopers punish overextension) | P1 |
| L08 | "Name the reward before you trade." | Health dropping in repeated frames with no soul gain, clock < 6:00 | C deadlockatlas trade rule | P3 |
| L09 | "Use the right-side peek." | Player in lane, taking damage while peeking left of cover (needs cover recognition) | W camera on right shoulder | P3 |
| L10 | "Stay in your lane until it's safe." | Clock < 8:00, player away from own lane, own lane losing | C metabot lane-hopping | P2 |
| E01 | "You have unspent souls. Go shop." | Unspent souls high relative to empty slots for 60s+, alive, no fight (threshold UNVERIFIED, no item prices fetched) | C hoarding warning | P2 |
| E02 | "New item slot unlocked. Fill it." | Flex slot icon unlocked and empty, souls available | W Extra Slots per Walker | P2 |
| E03 | "Unsecured souls. Don't die." | Haunted souls marker visible and large (above ~ 50+5/min), enemy near | W haunted souls lost on death (Souls) | P1 |
| E04 | "Spend them before the fight." | Large unspent souls while objective timer (Urn/Rift/Midboss) < 20s | C shop before big fights | P2 |
| E05 | "Take the Walker for a new slot." | Walker visible, flex slots still locked, clock > 12:00 | W Walker = slot + 3675 souls | P3 |
| E06 | "You keep dying. Add defense." | 2+ deaths in last 4 minutes and item list lacks defensive items (item recognition UNVERIFIED) | C metabot defensive item | P2 |
| E07 | "Counter what's killing you." | Damage report shows one source type repeatedly; no counter in items (UNVERIFIED feasibility) | W counterbuy section | P3 |
| E08 | "Ult is unlocked. Use it." | Souls >= 3.8k, ultimate icon unlocked and ready | W ultimate at 3.8k | P2 |
| E09 | "Shop closed here. Use another." | Guardian destroyed at current shop location | W shops close per Guardian | P3 |
| M01 | "Urn spawns soon. Clear your wave." | Clock = 9:30 (first), then every 5:00 | W Urn at 10:00, then every 5 min | P2 |
| M02 | "Urn is down. Contest or deny." | Urn icon on minimap, landing, clock 10:00 + n x 5:00 | W urn descends 12.5s | P2 |
| M03 | "Urn runner exposed. Cover them." | HUD shows urn carried by ally; enemy markers near route | W courier cannot parry/zip; melee/stun forces drop | P1 |
| M04 | "Deny the urn orbs." | Urn delivery, orbs on screen at drop point, enemy-colored | W urn souls deniable | P2 |
| M05 | "Rift announced. Group up." | Rift icon on minimap after global announcement; allies not near | W 20s from announcement to opening | P1 |
| M06 | "Take the Rift alone, not contested." | Rift circle with only allies inside, capture bar rising | W 12s capture, contested otherwise | P3 |
| M07 | "Midboss is up. Don't die alone." | Midboss minimap icon active (or respawn timer < 30s), player at low health/alone | W Strategy caution | P2 |
| M08 | "Midboss low. Get ready for Rejuv." | Midboss roar / below 50% (announcement) | W roars at 50% | P2 |
| M09 | "Punch the crystal. Three credits." | Rejuvenator landed (after 6s descent), allies near | W up to 3 credits, heavy melee | P1 |
| M10 | "Enemy is taking Rejuv. Contest it." | Enemy near crystal, allies alive near | W either team can steal | P1 |
| M11 | "They are dead. Take the Walker." | 3+ enemy respawn timers on top HUD, nearest enemy structure undamaged | C metabot objective conversion | P1 |
| M12 | "Guardian is open. Push the wave." | Clock >= 12:00 (Guardian resist gone), allied wave near Guardian | W Guardian loses resist at 12:00 | P3 |
| M13 | "Walker is open. Push as a team." | Clock >= 18:00 (Walker resist gone), wave at Walker | W Walker loses resist at 18:00 | P3 |
| M14 | "Enemy missing. Watch your back." | An enemy marker not seen on minimap for 20s+ while pushing | W glossary "missing" | P2 |
| T01 | "Group up. Don't fight split." | Fight began, allies spread across minimap, <3 near | C metabot group before committing | P1 |
| T02 | "Fall back. Fight is lost." | Allied dead count >= enemy dead + 2 in current fight | C deadlockatlas retreat together | P1 |
| T03 | "Stay behind your front line." | Own marker ahead of allies in a fight, health dropping | C metabot formation | P2 |
| T04 | "Focus the exposed one." | Enemy isolated low-health or far from team in view | C metabot target priority | P2 |
| T05 | "Switch target. They're shielded." | Enemy defensive buff icon (UNVERIFIED recognition) | C metabot, W Metal Skin counter | P3 |
| T06 | "Light melee. Heavy gets parried." | Player in parry stun, or enemy parry animation seen | W parry 2.75s stun, +25% dmg | P2 |
| T07 | "Don't chase. Take the objective." | After a fight win: enemies dead, structure in range, player chasing | C metabot/deadlockatlas | P2 |
| T08 | "Hold the high ground." | Fight about to start, enemy approaching, player in low open area | C metabot elevation | P3 |
| C01 | "Don't push in alone." | Clock > 20:00, all allies show respawn timers, player near enemy base | W respawn ~38s+ at 20 min (Death) | P1 |
| C02 | "Wave first, then siege." | Player at base structures with no allied troopers (backdoor "Protected" text) | W backdoor 65% resist, 65 regen | P2 |
| C03 | "Phase one down. Hit them fast." | Patron phase 1 destroyed (announcement/health bar) | W Last Stand shortens defender respawns | P1 |
| C04 | "Clear the pit first." | Weakened Patron, defenders in pit (shield/resist icon) | W 50%/100% resist with defenders in pit | P1 |
| C05 | "Rejuv credits left. Push now." | Top bar shows credits remaining and buff time < 60s | W buff 3 min | P1 |

## Counts and notes
- 47 rows; subset can be reduced to ~40 by dropping P3 rows (L09, L08, E07, E09, E05, T05, T08, M12, M13, L03) for a leaner set.
- UNVERIFIED thresholds: all "high unspent souls", "health < 35%", "20s+ missing" values are coach design choices to tune; they are not from sources.
- Hero-specific cues are intentionally omitted (no hero pages fetched).
- Cue text should avoid claiming exact numbers the sources dispute (e.g. Rift interval, Guardian soul value).
- Cues relying on item or buff recognition (E06, E07, T05) depend on the vision pipeline and are UNVERIFIED as feasible.
