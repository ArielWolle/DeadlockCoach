# Jungle Tactics: Invading, Counter-Jungling, Minimap Cues

Retrieved: 2026-10-25

Labels: [WIKI] wiki/API fact, [COMMUNITY] guide opinion, [DERIVED] my logic from facts, UNVERIFIED = no source.

Sources:
- https://deadlock.wiki/Strategy
- https://deadlock.wiki/Haunt
- https://deadlock.wiki/Minimap
- https://deadlock.wiki/Cursed_Apple
- https://deadlock.wiki/Zipline
- https://deadlock.wiki/Steam_Vent
- https://deadlock.wiki/Sinner's_Sacrifice
- https://api.deadlock-api.com/v1/assets/map
- https://mobalytics.gg/deadlock/guides/jungle-guide
- https://deadlockhandbook.com/souls/farming-routes/ and /jungle-farming/

## Invading / counter-jungling

### Stalling enemy respawns [WIKI Strategy + COMMUNITY Mobalytics]
- Camp respawn timer starts only when every Haunt (and every Sinner's Sacrifice in a hybrid camp) is dead. Leave one alive in enemy-side camps to deny their next spawn; fully clear camps on your own side.
- Haunts only chase a bounded distance from camp centre (Haunt page), so a leashed leftover Haunt resets and survives.

### Stealing [WIKI]
- Haunt souls go to the allies who damaged it within the last 5 s; last hit by one team gives the full reward to that team's last-hitting contributors (Haunt page examples). Killing a camp an enemy is mid-clear steals the souls.
- Heroes get hit by Haunt AoE/special attacks even if not the aggro target; a Haunt hitting another hero doesn't change aggro unless they damage it.
- Strategy: roaming assassin heroes can steal unsecured souls from junglers.
- Haunted souls (jungle) are spent after secured souls and can drop if you die; dying with an unsecured bank is costly (Handbook).

### Ambush spots [WIKI]
- Sinner's Sacrifice: you are locked in melee, taking 80 per hit; "may attract unwanted attention". Bell Tower: any damage rings the bell for everyone (can be rung remotely by shooting golden statues from distance as a decoy; barrier doesn't block projectiles, very little cover, 30 s cooldown). Sunken Plaza: fog blocks hearing and vision, drains stamina - hard to escape.
- Juke Rooms (Cosmic Veil doorway shadow) are dead ends for hiding/ambush; 30 veils in API.
- Steam Vents hide you from enemy heroes and untargetable by target abilities; revealed when you damage an enemy hero or one enters the vent.

### Mobility to enter/exit [WIKI]
- Zipline attach is blocked 3 s by enemy hero damage, not by Haunt damage. Riders hit by enemy heroes before 3 s of riding are knocked off (15% max HP + stun). Jump pad launch: damage in first 0.9 s stuns, slows 30% for 2 s, 15% max HP.
- Teleporters (2 pairs, 4 s channel) cross the map.
- Urn carrier cannot zipline/teleport.

### Priority guidance
- [COMMUNITY Handbook] Don't be farming when your team is grouping for Midboss/Rift/Urn/push. Farm toward the next objective.
- [WIKI Strategy] Midboss respawns on a shrinking timer (7 / 6 / 5 min per Cursed_Apple icon table; Strategy page says "every 6 minutes" - conflict); do not die while Midboss is up.
- [DERIVED] Dying wastes the 30 s wave; Strategy page: leave lane right after securing the wave.

## Vision and minimap cues a screenshot-based coach can detect

All items below are minimap/HUD observables; icon shapes come from the wiki Haunt/Minimap pages and the API icon URLs.

### Camp state [WIKI]
- Camp icon is a turquoise triangle; no lines = small, one line = medium, two thick lines = large; Sinner's Sacrifice camp uses a different (rectangle-with-circle) icon. API icon file names: `neutral_vault_psd.png`, `neutral_large_psd.png` under https://assets-bucket.deadlock-api.com/assets-api-res/icons/minimap/ (small/medium names UNVERIFIED but follow the pattern).
- A camp that is missing from the minimap on an expected spawn time means it was cleared (or not yet spawned - small before 2:00, medium before 5:00, large/vault before 8:00). [DERIVED]
- Wiki states the map shows the icons; it does not explicitly say icons disappear while cleared. UNVERIFIED that icons vanish after clear - verify in a capture before relying on it.

### Timer-based expectations [WIKI]
- Small respawn = 1:25 after full clear; medium 4:50; large 5:35; Sinner's 5:00.
- Powerup minimap icons show the type (Weapon/Movement/Spirit/Survival) at 5:00, 10:00 ... Both bridges spawn simultaneously. Uncollected powerups get replaced at next interval.
- Soul Urn: spawns at 10:00 with 12.5 s descent, moves on its own after 3 min, trackable on minimap.
- Rift: visual spawn indicator -> 40 s -> global announcement -> 20 s -> contest begins (Cursed_Apple timings, flagged outdated).
- Midboss respawn 7 / 6 / 5 min after kills.
- Healing Snacks, crates, Buff Containers are NOT on the minimap (Healing Snack page).

### Enemy-position cues [WIKI Minimap]
- Enemies appear on your minimap if: in an ally's line of sight within roughly 50-60 m, or shooting enemy Troopers / defensive structures, or being targeted by them, or very near an ally; stay visible about 2 s after.
- Not shown: standing near troopers without shooting, using abilities on troopers, being shot from beyond 50-60 m, Haze invisibility with Smoke Bomb, Calico under Ava, etc.
- Shooting summons/deployables does not reveal.
- Missing-enemy ping: Tab + middle-click on portrait.
- Absence of a hero from lanes ("missing") is a jungle/invade cue. [DERIVED] If a lane shows one fewer enemy on the minimap and nearby camps are cleared early, they are likely jungling or rotating.

### Other HUD cues [WIKI]
- The Bell Tower bell is audible to everyone - an audio cue, not a screenshot cue.
- Powerup icon appears under a hero's health bar when held; active effect icons on the left HUD, duration above soul count (Powerup page).
- Zipline boost: "Zipline Boosted" buff; express lines after Base Guardian destroyed.
- Respawn timers grow, ~35 s at 20 min (Strategy); Zipline Boost unlock at 6:00 means a freshly killed enemy returns faster.

### Heuristics for the coach (DERIVED, UNVERIFIED in play)
- At 5:00 / 8:00 flags: if the player is still in lane with no jungle activity while camps just spawned near their lane and the wave is pushed, suggest a camp.
- If a player is at a Sinner's Sacrifice with low health and an enemy is missing from the minimap, warn of ambush (wiki confirms retaliation damage and ambush risk).
- Souls bank: warn to shop before fights after a jungle run (Handbook).

## Known gaps
- No source gave exact per-camp ambush hot-spots, vision-ward rules (no wards documented), or which minimap icons vanish on clear.
- No source gave hero-specific jungle matchups beyond named examples in camp-types.md.

## Verified quotes for coaching cards (retrieved 2026-10-27)

Re-checked against the live wiki before building jungle advice; quoted verbatim.

- deadlock.wiki/Haunt, Overview [WIKI]: "Tier I camps spawn at the 2:00 minute mark, Tier II camps at 5:00, and Tier III camps at 8:00."
- deadlock.wiki/Haunt, Strategy [WIKI]: "Clearing camps on the opposite team's side of the map is rewarding as it makes it harder for the other team to gain souls, but it comes with the risk of being attacked and killed by opposing heroes."
- deadlock.wiki/Haunt, Strategy [WIKI]: "players tend to do so only when the zipline on a nearby lane is pushed into the opponent's side, and when they know where all the opposing heroes are."
- deadlock.wiki/Haunt, Strategy [WIKI]: "One common strategy involves killing every Haunt in an camp on the opponent side but one, which prevents the others from respawning."
- deadlock.wiki/Strategy, Jungling [WIKI]: "Clearing a trooper wave is generally faster and rewards more souls than a Neutral camp, so it's recommended to clear camps in between trooper waves."
- deadlock.wiki/Strategy, Timings [WIKI]: "If you plan on leaving lane, it is generally best to do so right after securing the wave."
- deadlock.wiki/Strategy, Jungling [WIKI]: "When jungling, players need to be careful to avoid roaming assassin heroes who might steal their unsecured souls."

Coach interpretation [DERIVED]: an enemy missing from your lane is a reason not to invade (they may be in
that jungle); invading is suggested only when the missing laner is dead or at most one enemy is
unaccounted for, matching "they know where all the opposing heroes are".