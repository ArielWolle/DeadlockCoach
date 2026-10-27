# Recorded match analysis 4: why you died, and how you spent your time

Grey Talon, 1920x1080, 600 frames covering game clock 16:10-38:00 (the recorder keeps the last 600
frames), ending in a Defeat. The player asked: why did I die, what should I improve, am I in the
jungle too much, am I letting my team down, should I be doing other things. Two independent
reviewers answered from the screenshots (one death by death, one on macro play); the coach was then
taught to give the same kind of advice live.

## The match

| Death | What happened (screenshots) | Main lesson |
| --- | --- | --- |
| 24:31-24:46 | Pushed into the enemy half of Green with 4 enemies unseen; Bebop had just killed two teammates; Hyper Beam then Sticky Bomb, 2,417 -> 0 in ~10 s; 3,019 souls unspent | Don't cross into their half with enemies missing; leave when teammates fall in front of you |
| ~26:30 | Mid-pit fight, stood on the stairs in melee range; melee diver; 2 enemies unseen for 30+ s | Fight from the rim at max range behind your front line |
| 28:06-28:15 | Respawned and walked to Green alone; allies 0.3-0.4 map away; two enemies; 2,200 -> 0 in ~9 s | After a respawn, group or catch the wave under your guardian |
| 30:40-30:48 | Base defence in the open; melee + Bashdown burst, 2,252 -> 0 in ~8 s | Shoot from behind structures; save escapes for the diver |
| 35:54-36:10 | Carried the Soul Urn alone without a front line; intercepted; 78 s respawn decided the game | Let a tankier ally carry the Urn; no solo jobs late |

- **Jungle:** not too much. 7-17% of readable alive time; lanes were 72%. Jungle time during team
  fights was about zero.
- **Team:** joined 9 of 14 fights, 2 late, 2 missed cross-map, 1 missed while dead. You were not farming
  while they fought; the damage to the team came from dying: 5 deaths in 13.5 minutes, about 25% of
  the time dead, at least two of them solo.
- **Souls:** over 3,000 unspent for about 290 s (30% of alive time); 1,376-3,019 unspent at every death.
- **Other things to do:** spend at 3k before going back in; push the deepest wave when two or more
  enemies are dead (20:24, 32:07 were missed); catch waves near base instead of camps when every lane
  is pushed in (33:36).

## What the coach now says

A new review coach (`crates/engine/src/review.rs`, cards with document sources in
`data/coaching/review.json`) keeps the last minutes of readings:

- **On the death screen, once per death:** why you died, from the 15 s before your last alive reading:
  how fast you fell, teammates that died first, walking out alone after a respawn, no teammate near,
  being in their half with enemies unseen, the same kind of damage killing you again, unspent souls,
  and the damage panel's cause. Then one or two lessons. It replaces the single "Enemy heroes killed
  you" sentence that repeated during a death; Midboss / Walker / structure / trooper advice is kept.
- **Every five minutes (while waiting to respawn, or in a calm moment):** the play pattern: deaths
  and time dead, team fights more than a third of the map away from you, jungle share over 50%,
  waves left deep on your side while you were away, time holding over 3,000 souls; or that your
  balance was good.
- **Live:** "N teammates just died near you. The fight is lost: back off now." and, within 40 s of
  a respawn, "N enemies are unseen and no teammate is near. Walk out with a teammate, not alone."

Replay of this recording with the new coach:

- 24:46 "Why you died: You went from 73% to dead in 7 seconds. You were in their half with 4
  enemies not seen by your team. You died holding 3019 souls. Damage panel: Hyper Beam. Next time:
  Leave on the first big hit, not at low health, and stay behind your front line. When enemies are
  missing, don't push past your wave."
- 28:17 "Why you died: You walked out alone 30 seconds after respawning. ..."
- 30:52 "... That is 2 deaths to melee hits. ... Check what is killing you and buy a counter or a
  defensive item."
- 31:06 "Last 5 minutes: you died 4 times and were dead for 2.2 minutes. Play for zero deaths: fight
  from behind your front line."
- 32:07 "2 teammates just died near you. The fight is lost: back off now."

## Pipeline fixes found here

- From 34:41 every frame was Unknown: the base under attack failed the minimap's white home-base
  landmark, so no game context formed and the coach went silent for the last death. The last
  landmark-verified map box is now held (template score >= 0.2; normal 0.52-0.61, late fights
  0.26-0.37, end screen 0.0) and a differently sized find cannot replace it. Gameplay frames 333 -> 433.
- A death review could lose its speaking slot to the dead-with-souls reminder; it is now offered on
  every death-screen frame under a per-death key and ranked first.
- Damage-panel causes are cleaned of the "Killing Blow" tag and its misreadings and of hit counts.

## Known limits

- The killer's name and abilities on the panel's Killing Blow row are not read; only the top row.
- Respawn timers misread on some death screens; spectating after death reads as Unknown.
- The Urn-carry and hero-specific (Grey Talon range) advice need hero and HUD-text reads not yet built.
