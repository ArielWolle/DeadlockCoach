# Coach review 16/17: the death-screen damage panel

User report: "the damage taken after death is sometimes wrong. Make sure the damage taken screen is
received well and disregarded [when it cannot be read], or the user is told to open it if it is
closed."

Recordings: `frames/rec16` (loss, Grey Talon, 600 frames 12:48-38:02) and `frames/rec17` (win,
10:46-32:53), with `frames/rec8` and `frames/rec13` as regression. Ground truth comes from the
1920x1080 frames (panel crops); "live" is the snapshot the coach had during the match, "replay
before" is the vision of `8d07831` (`replay-base.jsonl`), "after" is this branch (`replay_dir`, then
`active_replay --all` and `web/scripts/ai-coach-eval.ts`).

## What the panel does on screen

- **Open**: "INCOMING DAMAGE (LAST DEATH)", up to six rows (hero portrait, source, hits, damage),
  "Total Damage: N" and a Show More button. The red-orange **Killing Blow** tag is drawn over the
  name of the tagged row and hides part of it ("Lurkin…", "Stalk…", "Borrow…", "Spirit R…").
- **Collapsed (closed)**: only a dark square with your own hero's portrait at the right edge,
  where the open panel has its header and X button (1920x1080: x 1852-1919, y 250-320). No key is
  shown for it on screen and none is documented in the repo, so the coach says "click your hero
  portrait at the right edge of the death screen".
- The panel is **collapsed for the first seconds of every death** (rec8 20:40 and 49:42: tab on
  the first frame, rows ~5 s later) and opens by itself; rec16 27:49 and rec13 19:33 stayed
  collapsed for the whole death. Rejuvenator revive screens show the tab too.
- The panel **changes during the death**: the numbers count up for a few seconds (rec16 24:03:
  3,468 for 3 s, then 6,295), and the game rewrites it later (rec16 32:33: 2,929 -> 5,963 22 s
  in, Royal Pestments on top; rec16 36:57: a 5,352 panel with two Killing Blow tags covering the
  36:44 revive and the death, replaced 32 s in by 2,969 for the death alone; rec13 41:05: 5,198 ->
  25,010 at 42:25).

## Per-death table

Clock = first death-screen frame. Truth = the panel on screen (the state the review could use).
Verdicts are for the live / replay-before read.

### rec16 (8 dead-screen transitions = 4 deaths + 1 revive + 3 re-entries after flicker frames)

| Death | Panel | Truth | Read before (live / replay) | Spoken before | Verdict before | After |
| --- | --- | --- | --- | --- | --- | --- |
| 24:03 | open | Melee 1,319 · Gun Damage 1,067 · Gun Damage 808 · Ira Domini 534 · Ira Domini 484 · Melee Charge 106 (KB) · total 6,295 (3,468 while counting up) | rows right, 3,468 then 6,295; **no killing blow** | rule: "Silver and Paige fell and you stayed in"; AI: "Slowed and out of stamina in melee range…" | wrong (KB missing) | same rows, KB **Melee Charge**, the counting-up 3,468 panel never reported; review unchanged, AI prompt has the KB |
| 27:49 | **closed** whole death | nothing to read | nothing | rule: "burst you from 61% in 11 seconds" (HP based); AI: PASS "cause unknown" | correct, but the player was never told to open it | `death_panel` "closed" from 27:55; **27:58 "Open the damage panel: click your hero portrait at the right edge of the death screen, so I can tell you what killed you."**; review waits 20 s (28:10, no damage claim); AI prompt "damage panel: NOT VISIBLE … name no damage source" |
| 32:33 | open | Melee 537 · Royal Pestments 451 · Gun Damage 444 · Siphon Life 417 · Lucky Shot 225 · Stalk… 109 (KB; Abrams' row) · 2,929; from 32:59: Royal Pestments 868 · Gun Damage 845 · 748 · 674 · Melee 537 · Stalk… 109 · 5,963 | live: top row Melee missing, cause "Melee" then "Royal Pestments", no KB, stuck at 2,929; replay: Lucky Shot **22**, no KB, stuck at 2,929 | rule: "Rat King's Royal Pestments and Abrams focused you"; AI: "Rat King's melee killed you" | wrong rows / wrong numbers (AI blamed Rat King for Abrams' melee) | all 6 rows right (225; "Stalk…" = Stalker by elimination: no Drifter in the match), KB Stalker; 5,963 panel confirmed at 32:59; rule: "Abrams' Siphon Life and Rat King focused you" |
| 36:44 | tab (revive) | Rejuvenator revive | revived | nothing | correct (not a death) | unchanged |
| 36:57 | open, 2 KB tags | Lurking Terrors 3,142 (KB) · Escalating Exposure 761 · Spirit Burn 647 · Torment Pulse 285 · Gun Damage 229 · Leech Therapy 65 (KB) · 5,352; from 37:29: Lurking Terrors 1,496 · 453 · 441 · 285 · 229 · 65 (KB) · 2,969 | **top row Lurking Terrors missing** (tag over its name), cause "Lurkin" / "Escalating Exposure", KB Leech | rule: "burst you from 74%"; AI: "Leech and Escalating Exposure finished you at 18%" | wrong rows (59 % of the listed damage missing) | top row Lurking Terrors 3,142, no KB while two rows are tagged, then the 2,969 panel with KB Leech; rule: "Nurse Harrow's Lurking Terrors burst you from 74% in 14 seconds" |

The 8 transitions: 24:03, 27:49, 28:52 (back from a Scoreboard frame, same death), 32:33, 36:44
(revive), 36:57, 37:06 and 37:22 (back from Shop frames read `dead: false`, same death). Every
real death had exactly one review before and after; the revive had none. Nothing was missing.

### rec17

| Death | Panel | Truth | Read before | Spoken before | Verdict | After |
| --- | --- | --- | --- | --- | --- | --- |
| 9:11 | not in the 600 frames | the AI digest: Familiar 588 · Gun Damage 395 · Assassinate 360 · 4,150, KB Assassinate | - | - | not verifiable | - |
| 26:12 | tab (revive) | Rejuvenator revive ("Respawn in: 2", grey world, REVIVED! at 26:19; scoreboard deaths stay 1) | `revived` | **rule death review at 26:35: "You died because you stayed deep in their half after the call to get out"** | wrong (a revive got a death review: live frames without a health read kept the death open) | no death, no review, no AI death |

### rec8 (regression)

| Death | Panel | Truth | Replay before | Verdict | After |
| --- | --- | --- | --- | --- | --- |
| 17:00 | open | Melee 389 · Melee 239 (KB) · Enchanter's Satchel 220 · Flying Cloak 208 · Flying Slash 193 · Gun Damage 177 · 1,941 | right | correct | right |
| 20:40 | tab, then shop, then open at 21:10 | Tankbuster 479 · Barrage 459 · Flying Slash 426 · Gun Damage 408 (KB) · Arctic Beam 294 · Jettison Fuel 279 · 4,317 | 5 rows (KB row missing), no KB | wrong rows | not confirmed (the vision resets the panel after 15 s without a timer read between the two reads): disregarded, review without a damage claim (same text) |
| 27:57 | open | Melee 525 · Flying Slash 501 · Alt Gun Damage 389 · Melee Charge 131 (KB) · Express Shot 124 · Power Slash 121 · 1,893 | Melee Charge read "Melee", KB "Melee" | wrong row name | Melee Charge (KB), "Gun Damage 389" for Alt Gun Damage |
| 39:17 | revive | - | revived | correct | unchanged |
| 42:05 | open after 25 s (shop) | Arctic Beam 1,372 (KB) · Affliction 1,309 · Spirit Burn 611 · Barrage 473 · Tankbuster 444 · Flying Cloak 399 · 7,160 | **mixed rows of two reads** (Arctic Beam 1,342 + the 3,048 filling panel) | wrong rows / numbers | all 6 rows, 7,160, KB Arctic Beam; the review waits for the panel (42:30) |
| 46:43 | open | Affliction 2,172 · Spirit R… 1,104 (KB) · Frost Grenade 1,017 · Escalating Exposure 873 · Enchanter's Satchel 494 · Flying Cloak 399 · 7,307 | "Spirit Armor" guessed for the tagged row, KB Spirit Armor | wrong name | the ambiguous "Spirit R…" row dropped (Spirit Rend / Resilience), no KB, the other 5 rows |
| 49:42 | tab, shop, open | Bullet Dance 2,017 (KB) · Fixation 932 · Capacitor 398 · Scrap Grenade 317 · Royal Pestments 122 · Ricochet 61 · 3,931 | rows right, no KB | KB missing | KB Bullet Dance (Haze is in the match); review "Haze's Bullet Dance caught you" at 50:21 |

### rec13 (regression)

| Death | Panel | Truth | Replay before | Verdict | After |
| --- | --- | --- | --- | --- | --- |
| 6:07 | open | Specimen II 213 · Rake 189 · Essence Bomb 179 (KB) · Mystic Burst 57 · Gun Damage 46 · Life Drain 28 · 733 | top row missing, cause **"Spcimen Yo"**; review "…against Lady Geist" | wrong (garbage cause) | "Specimen II" is not a known source: disregarded; review at the respawn: "you stayed in under 40% health" |
| 19:33 | **closed** whole death | - | nothing | correct, never told | tip at 19:47, review 19:57 |
| 23:09 | open | Borrow… 672 (KB: Borrow Life or Borrowed Decree, both possible in this match) · Grasping Hands 426 · Underhand II 340 · Jar of Dead 321 · Essence Bomb 306 · Knocked off Zipline 277 · 2,723 | Essence Bomb **106**, Tankbuster 70 / Gun Damage 62 from the filling panel, no total | wrong numbers / rows | top row ambiguous: `unreadable`, disregarded |
| 29:26 | open | T2 Walker 1,045 · Afterburn 869 (KB) · Kinetic Carbine 678 · Jar of Dead 484 · Grasping Hands 439 · Toxic Bullets 325 · 6,097 | "Walker", rest right | correct (Walker for T2 Walker) | same |
| 41:05 | open | Melee 2,322 (KB) · Gun Damage 979 · Torment Pulse 576 · Siphon Life 548 · Spirit Snatch 288 · Crushing Fists 267 · 5,198; 25,010 from 42:25 | right (5,198) | correct | right, then the 25,010 panel |
| 43:06 | open | Afterburn 1,756 · Toxic Bullets 899 · Escal… 445 (KB) · Gun Damage 392 · Napalm 224 · Mercurial Magnum 133 · 3,952 | 5 rows, no KB | correct rows, KB missing | same (the tagged "Escal…" is Exposure or Resilience: dropped, no KB) |

## Causes

1. **3 and 5 lost digits.** The digit-cell glyphs used the strict white test; at 1080p the thin
   middle stroke of "3" and "5" falls under it, the glyph splits into two halves that the height
   filter dropped, and the crop missed the digit: 225 -> "22", 3,142 -> "142", 501 -> "O1", 340 ->
   "440", 321 -> "221", 392 -> "92", 213 -> "21".
2. **The Killing Blow tag** was searched in a fixed narrow box with a pure-red test (hue < 12):
   tagged rows measured 1-27 px against a threshold of 15. The tag is red-orange (hues 5-20; 345-15
   over purple bars) and sits wherever the name ends.
3. **Names under the tag** ("Lurkinling:Blow", "Borrowiiling:Blow", "Bullet Rling Blow") were not
   stripped, so the tagged row, often the top one, was dropped and the second row became "the
   cause"; ambiguous clips were guessed ("Spirit R" -> Spirit Armor).
4. **No reliability gate**: any single read was reported, rows of different panel states were
   merged, a partial read without the top row still produced a cause (the detector's top-line read
   "Spcimen Yo", "Melee", "Lurkin"), and the panel stopped being read 8 s after its total repeated,
   so the game's later rewrite was never seen.
5. **Revive**: the engine ignored `revived`; after a revive the live frames without a health read
   were kept as part of the death (rec17 26:12).
6. **Closed panel**: not detected; no tip.

## Fixes

Vision (`crates/vision/src/death.rs`, `scan.rs`):
- Digit glyphs use a looser white (v > 0.6, s < 0.3): all the numbers above read right.
- Killing Blow: red-orange pixels (hue < 22 or > 340, s > 0.55, v > 0.6) across the whole name bar
  right of the icon, >= 90 px (tags 160-400, other rows 0-63). Two tagged rows: no killing blow.
- Tag stripping handles "…ling Blow", "…lingBlow", "…ngBlow"; a clipped name shared by several
  sources is never guessed: only this match's enemy team (top bar) settles it (an ability of a hero
  not in the match is impossible; a lone enemy ability beats shop items). A dropped real word costs
  more than a clip ("Leech Ther" is Leech Therapy); a name cut at both ends matches inside the name.
- **Reliability gate** (`PanelRead::complete` + `Panel`): a read is complete when the top row is
  read, at most 2 other located rows are missing, Total Damage is read and the listed damage does
  not exceed it; rows are reported only when a second complete read at least 4 s later agrees
  (same total and top row, every row both read equal). A later state replaces it the same way; the
  panel is re-read every second until confirmed and every 4 s after. Nothing carries over to the
  next death (reset on the live HUD, a new countdown, or 15 s without a timer read).
- New snapshot field `death_panel`: "open" | "reading" | "unreadable" (rows on screen, nothing
  confirmed for 8 s over 3+ reads) | "closed" (the portrait tab with no rows, 6 s or more into the
  death). Unless "open", no `death_damage` / `death_killing_blow` / `death_total_damage` /
  `death_cause` is reported (the detector fallback for the top line is gone).

Engine (`crates/engine/src/review.rs`, `lib.rs`):
- The review uses the panel only when "open" (snapshots without the field are trusted), never a
  panel equal to the previous death's (stale), and waits up to 40 s while the panel is "reading",
  but never past a respawn timer of 4 s.
- Closed for 2 frames over 3 s: once per match, High, `review:panel_closed` (card
  `review.panel_closed`, skill threat_tracking). The review then waits the usual 20 s (the player
  may open it); on a later closed death it goes out at 8 s.
- A frame flagged `revived` is not a death (the LifeTracker hides it from the rules, and undoes a
  death the flag arrives for within 10 s; the review coach drops it).

AI (`web/src/ai-coach.ts`, `ai-review.ts`, `types.ts`): the death record takes rows / cause / killing
blow only from an "open" panel and never a repeat of the previous death's; the Death line says
"damage panel: NOT VISIBLE (collapsed on the death screen), so what killed the player is unknown:
name no damage source, ability or killer" (or "on screen but not readable"), and the death task
says the cause is then unknown. A revive is not a death. The end-of-game digest says "damage panel:
not visible (collapsed on the death screen), damage cause unknown".

## Before / after

| | before | after |
| --- | --- | --- |
| rec16 deaths with every row and number right at review time | 1 of 3 open panels (24:03, KB missing) | 3 of 3 (24:03, 32:33, 36:57) |
| rec16 killing blow named right | 0 of 2 single-tag panels (36:57 named one of its two tags) | 2 of 2 (Melee Charge, Stalker); none claimed for the two-tag panel |
| rec16 37:02 / 32:44 AI death prompt top source | Escalating Exposure 761 / Lucky Shot 22 in the rows | Lurking Terrors 3,142 / Lucky Shot 225 |
| closed-panel tip | never | rec16 27:58, rec13 19:47 (once per match) |
| rec17 death reviews | 1 (for a revive) | 0 (the only death, 9:11, is before the frames) |
| rec8 open panels read right (rows + KB) | 1 of 6 | 4 of 6 (27:57 with Alt Gun Damage as "Gun Damage"; 20:40 disregarded; 46:43 ambiguous row dropped, no KB) |
| rec13 garbage / wrong-number causes | 2 (6:07 "Spcimen Yo", 23:09 Essence Bomb 106) | 0 (both disregarded) |
| rule death reviews (rec16 / rec17 / rec8 / rec13) | 4 / 1 / 6 / 6 | 4 / 0 / 6 / 6 |

AI death prompts (`ai-coach-eval.ts`, rec16 after):
- 24:12 "cause Melee · damage taken: Melee 1,319, Gun Damage 1,067, … Melee Charge 106 · killing
  blow Melee Charge"
- 28:19 "cause unknown · damage panel: NOT VISIBLE (collapsed on the death screen), so what killed
  the player is unknown: name no damage source, ability or killer"
- 32:44 "Melee 537, Royal Pestments 451, Gun Damage 444, Siphon Life 417, Lucky Shot 225, Stalker 109
  · killing blow Stalker"
- 37:18 "cause Lurking Terrors · damage taken: Lurking Terrors 3,142, Escalating Exposure 761, …";
  "Damage by hero: Nurse Harrow 3,142 (Lurking Terrors)"

## Known limits

- The game rewrites the panel up to ~30 s into a death; the review (5-20 s in) uses the confirmed
  panel of that moment (rec16 32:44: 2,929, the rewrite to 5,963 came at 32:59).
- A tagged top row whose clip fits several sources of this match (rec13 23:09 "Borrow…") or a
  source missing from data/abilities.json (rec13 6:07 "Specimen II") makes the whole panel
  unreadable: disregarded rather than guessed.
- 1600x900 panels lose the "T2" of "T2 Walker" (no icon, the row starts under the portrait).
- Shop frames read `dead: false` on rec16 37:06 / 37:22 (the shop's respawn banner not found):
  still the same death for the engine and the AI; not changed here.

Fixtures: `samples/frames/00590`-`00597` (`crates/vision/tests/death_panel.rs`); engine tests in
`crates/engine/tests/review.rs` (open, closed + one tip per match, unreadable, stale, revive); web
tests in `web/tests/ai-coach.unit.ts` and `ai-review.unit.ts`.
