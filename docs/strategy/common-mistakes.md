# Common Mistakes (coach-detectable)

Retrieved: 2026-10-25

Source:
- https://metabot.gg/en/deadlock/guides/common-deadlock-beginner-mistakes [COMMUNITY]
- https://games.gg/deadlock/guides/deadlock-laning-guide/ [COMMUNITY]
- https://metabot.gg/en/deadlock/guides/deadlock-laning-phase-strategy-guide [COMMUNITY]
- https://metabot.gg/en/deadlock/guides/deadlock-team-fighting-positioning-guide [COMMUNITY]
- https://www.deadlockatlas.com/en/guides/deadlock-laning-guide , /deadlock-teamfight-guide , /deadlock-map-objectives-guide [COMMUNITY]
- https://deadlock.wiki/Strategy , /Souls , /Death , /Soul_Urn , /Midboss , /Rejuvenator , /Unstable_Rift , /Extra_Slots , /Melee_Attack [WIKI]

Legend: Each mistake lists **Basis** (WIKI fact vs COMMUNITY opinion), **Detection signal** (what a screenshot can show: game clock, souls, item list, minimap, health/stamina, timers), **Confidence** (how reliably a single frame shows it), and **Cue** (suggested spoken text; see coach-cue-library.md for IDs).

Detection signals are [INFERENCE]; none of the sources describe screenshot detection. Single screenshots cannot show intent; use two or more frames when marked "multi-frame".

---

## 1. Lane-hopping / leaving lane for nothing (early)
- Basis: [COMMUNITY] metabot: every second jogging earns zero souls and gives free guardian damage; rotate only with a reason. [WIKI] Strategy: leave right after securing the wave, return before the next arrives.
- Detection: clock < ~8:00; player marker away from own lane on minimap; own-lane trooper cluster near own Guardian; souls flat across frames. Multi-frame.
- Confidence: medium.
- Cue: "Your lane is getting pushed in. Get back and clear the wave."

## 2. Leaving lane on a neutral wave (no crash first)
- Basis: [COMMUNITY] games.gg ("wave tax"), metabot (shove before leaving).
- Detection: player leaves lane while minimap troopers are near mid-lane (even). Multi-frame.
- Confidence: low-medium.
- Cue: "Push the wave in before you leave, or you pay for it."

## 3. Over-brawling / trading without a reward
- Basis: [COMMUNITY] metabot, deadlockatlas ("name the reward"), games.gg (lane checklist).
- Detection: health bar < 50% while on lane at < 6:00 and no kill/souls increase; enemy hero health also high. Multi-frame.
- Confidence: low.
- Cue: "Don't trade unless it wins you a last hit or a kill. Back off and farm."

## 4. Missing last hits / not denying
- Basis: [WIKI] floating orbs can be shot (confirm/deny); [COMMUNITY] denies are a double swing.
- Detection: lit enemy-colored (orange) floating orbs on screen unshot; green orbs unshot; souls total lagging expected pace (no benchmark source - UNVERIFIED).
- Confidence: medium for visible orbs.
- Cue: "Shoot the orange orbs. Deny them."

## 5. Dying with haunted (unsecured) souls
- Basis: [WIKI] haunted souls are lost on death and drop in a Soul Bag only above 50 + 5/min.
- Detection: HUD souls with red haunted icon high, then death screen/grey screen. Single frame.
- Confidence: medium.
- Cue: "You are carrying unsecured souls. Stay safe or spend them."

## 6. Hoarding souls (unspent)
- Basis: [COMMUNITY] metabot/deadlockatlas (small early purchases often better than waiting; don't hoard while dying).
- Detection: unspent souls well above cheapest relevant item price, alive, shop reachable; multi-frame (same number rising 60s+). Price data UNVERIFIED (no item data fetched), so use "souls keep rising with empty slots".
- Confidence: medium.
- Cue: "You have unspent souls. Go shop when the wave is safe."

## 7. No defensive/sustain item (glass cannon)
- Basis: [COMMUNITY] metabot (greedy all-damage builds). [WIKI] counterbuy examples exist.
- Detection: item list at 10+ min contains no vitality-type items (needs item recognition) and health bar low often; UNVERIFIED item categories for recognition.
- Confidence: low.
- Cue: "You keep dying. Add a defensive item."

## 8. Not countering the enemy build
- Basis: [WIKI] Strategy counterbuying section; [COMMUNITY] metabot (buy for the actual threat).
- Detection: scoreboard/enemy items visible via Tab; dying repeatedly to same damage type. Requires item and damage report recognition. UNVERIFIED feasibility.
- Confidence: low.
- Cue: "Check what is killing you and buy a counter."

## 9. Unspent flex slot after Walker
- Basis: [WIKI] extra slots unlock per Walker; [COMMUNITY] high value.
- Detection: new flex slot icon unlocked (empty) with souls available. Single frame.
- Confidence: medium.
- Cue: "New item slot unlocked. Fill it."

## 10. Chasing kills instead of taking objectives
- Basis: [COMMUNITY] metabot, deadlockatlas ("convert, then reset").
- Detection: enemy team dead/respawn timers on top HUD; player far from nearest enemy structure; Guardian/Walker at full health. Single frame.
- Confidence: medium.
- Cue: "They are dead. Take the Walker now."

## 11. Ignoring waves before an objective
- Basis: [COMMUNITY] deadlockatlas.
- Detection: before Urn/Rift/Midboss timers, own lane trooper cluster is close to own Guardian while the team moves to objective. Multi-frame.
- Confidence: low-medium.
- Cue: "Clear your wave first, then go to the objective."

## 12. Dying while Midboss/Rejuvenator is up
- Basis: [WIKI] Strategy: care not to die when Midboss is (soon) available. Midboss icon shows on minimap while alive.
- Detection: Midboss icon active/respawn timer near zero; player death or low health alone. Single frame.
- Confidence: medium.
- Cue: "Midboss is up. Don't die alone."

## 13. Starting Midboss without vision/numbers
- Basis: [COMMUNITY] map objectives guide (plan before, assign jobs); [WIKI] only players inside pit can hurt it; either team can steal the Rejuvenator.
- Detection: minimap shows only 1-3 allies near the pit while enemies unaccounted for. Single frame.
- Confidence: low-medium.
- Cue: "Group up before Midboss. Where are the enemies?"

## 14. Contesting Unstable Rift alone or late
- Basis: [WIKI] Rift needs a lone team in the 20m circle; 12s capture; both teams earn progress; revealed on minimap.
- Detection: Rift icon active at spawn; clock ~ announcement + 20s; player not near. Single frame.
- Confidence: medium.
- Cue: "Rift is open. Group up and take it."

## 15. Carrying the Urn into danger / not escorting
- Basis: [WIKI] courier cannot parry/zipline; melee/stun drops the urn; decays after 45s.
- Detection: player carrying urn (HUD "delivering the urn" text); enemies near on minimap. Single frame.
- Confidence: medium.
- Cue: "Urn runner is exposed. Back them up."

## 16. Fighting split / piecemeal
- Basis: [COMMUNITY] metabot, deadlockatlas.
- Detection: minimap shows allies spread; one ally dead already; fight underway. Single frame.
- Confidence: medium.
- Cue: "Fall back and regroup. Don't fight split."

## 17. Standing in front of the tank (inverted formation)
- Basis: [COMMUNITY] metabot formation.
- Detection: own marker ahead of allies in a fight; own health dropping. Hero role not knowable from the screenshot (UNVERIFIED).
- Confidence: low.
- Cue: "Hold back behind your front line."

## 18. Chasing into the enemy base / past safe zone
- Basis: [COMMUNITY] metabot "don't throw by chasing one last kill". [WIKI] base turrets and shrines, long respawn late.
- Detection: late clock; player near enemy base; allies not near; own health low. Single frame.
- Confidence: medium.
- Cue: "Stop chasing. Reset and take the objective."

## 19. Engaging without an advantage / continuing after opener whiffs
- Basis: [COMMUNITY] metabot.
- Detection: fight in progress with allies dead more than enemy dead. Single frame (kill feed/portraits).
- Confidence: medium.
- Cue: "Fight is lost. Retreat together."

## 20. Shooting a defended target (Metal Skin etc.)
- Basis: [COMMUNITY] metabot; [WIKI] Metal Skin is a counter item.
- Detection: requires buff icon recognition on the enemy. UNVERIFIED.
- Confidence: low.
- Cue: "Switch target. They are shielded."

## 21. Being parried by heavy melee
- Basis: [WIKI] heavy melee wind-up can be parried; parry stun 2.75s and +25% damage.
- Detection: parry stun status icon on player. Single frame.
- Confidence: low-medium.
- Cue: "Don't heavy melee into a parry. Use a light melee."

## 22. Ignoring the minimap / missing alerts
- Basis: [WIKI] glossary "missing"; [COMMUNITY] map awareness.
- Detection: enemy markers disappear from the minimap while the player pushes forward; multi-frame.
- Confidence: low-medium.
- Cue: "Enemy is missing. Watch your back."

## 23. Jungling in enemy reach while holding sacrifices
- Basis: [WIKI] roaming assassins can steal unsecured souls; Sinner's Sacrifice souls are haunted; melee retaliation 80 damage.
- Detection: player at Sinner's Sacrifice location; clock > 8:00; enemy hero blinking on minimap nearby. Single frame.
- Confidence: low.
- Cue: "You are farming risky souls. Check the minimap."

## 24. Late-game solo splitting while the team is dead
- Basis: [WIKI] respawn 38s+ at 20 min; [INFERENCE] risk.
- Detection: teammates all show respawn timers while the player is far forward. Single frame.
- Confidence: medium.
- Cue: "Your team is dead. Don't push in alone."

## 25. Spending too long in a dead lane
- Basis: [COMMUNITY] metabot: "Sitting in a dead lane long after the map has moved on".
- Detection: after ~8:00 minimap shows no enemy near player's lane; teammates elsewhere. Multi-frame.
- Confidence: low-medium.
- Cue: "Lane is dead. Shove it and go help."

---

## Not coach-detectable from screenshots (documented so we do not claim them)
- Aim quality, parry timing, dash/stamina micro (needs video).
- Item choice quality for a specific hero (needs hero/item data; none fetched).
- Chat/voice behavior (throwing/intentional feeding per Glossary).
