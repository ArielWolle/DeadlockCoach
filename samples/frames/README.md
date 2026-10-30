# Full-frame regression fixtures

These screenshots are versioned inputs for `crates/vision/tests/frames.rs` and the browser
OCR/capture tests. Keep this directory in Git; a clean CI checkout must contain the images.
Local capture/debug output belongs in the repository-root `frames/` or `sessions/` directories.

| Prefix | Fixture purpose |
| --- | --- |
| `00014` | Normal gameplay |
| `00048` | Shop view |
| `00049` | Gameplay near the shop |
| `00059` | Clock, souls, team totals and inventory regression |
| `00167` | Menu detection |
| `00184` | Scoreboard recognition |
| `00216` | Death/respawn screen |
| `00262` | Midboss-related death cause |
| `00400` | Top-bar health: all twelve bars full at 0:43 |
| `00401` | Top-bar health: damaged, CRITICAL, not-recently-seen (no bar) and dead (greyed, respawn timer) heroes at 4:53 |
| `00500`-`00504` | 1920x1080 recorded match (`docs/recording-analysis-1.md`): truncated HP number, mirrored lane layout, pale unseen enemy, `$` read as a digit, real low HP |
| `00505`-`00508` | 1920x1080 recorded match rec5 (`crates/vision/tests/topbar_rec5.rs`): death screen with the top bar drawn 7 px low and five respawn timers, 3-digit team totals after a recording gap, Tab scoreboard K/D/A, four dead slots (dark portraits) and all twelve per-hero labels |
| `00520`-`00526` | 1920x1080 recording rec5 (`crates/vision/tests/hud_status.rs`): souls 2,809 / level 17, shop open while alive, STUNNED + Aura of Suffering badge, full death panel, respawn 30 (read as 0 before), shop open while dead (respawn 60), Defeat banner |

Only the explicit fixture allow-list in `web/scripts/assets.mjs` is included in the static
website artifact. This directory is not a recording destination.
| `00530`-`00531` | rec6: death screen whose spectator minimap is ~2% smaller, then gameplay; the latched smaller box made every later frame Unknown (`crates/vision/tests/automatic.rs`) |
| `00550`-`00554` | rec8 ground-truth review (`crates/vision/tests/rec8_review.rs`): clock 15:31 whose leftmost box reads 5:31, "DELIVER THE URN" with no minimap vase, AFFLICTED, Frost Grenade slow badges (-1.0 m), Hidden King health banner at ~50% |
| `00560`-`00561` | Match boundary pair (`crates/vision/tests/match_boundary.rs`): rec11 match A at 36:58 (Violet, ally slot 2) and rec12 match B at 0:22 (Grey Talon, ally slot 4; Billy's dark portrait in slot 0); both matches read the same HUD match id |
| `00570`-`00573` | rec13 end of match (`crates/vision/tests/hud_status.rs`): "Defeat" over the death screen ("Respawn in 1") and over the Tab scoreboard, the post-game summary ("ESC BACK" + PLAY AGAIN, was Shop), and a real shop of the same match |
| `00574` | rec12 end of a won match: "Victory" banner (`crates/vision/tests/gamestate.rs`: `match_result` "win"; 00526/00570/00571 are the "loss" cases, and 00508/00520/00531/00550/00551/00553/00554/00560 the ability-bar ground truth) |
| `00540`-`00542` | rec8 top-bar strips (1920x84, top of the frame; `crates/vision/tests/hero_identity.rs`): Grey Talon carrying the urn (mint square, other art; read Silver), Baba carrying the urn (read Solomon) and Bebop's turned portrait (read Solomon) |
| `00590`-`00597` | Death damage panel (`crates/vision/tests/death_panel.rs`, `docs/coach-review-16-17-panel.md`): rec16 24:08 open panel (6 rows, Killing Blow on Melee Charge, 6,295), 27:52 and rec13 20:03 collapsed to the portrait tab (closed), rec16 37:02 two Killing Blow tags (panel spanning a revive and the death; top Lurking Terrors 3,142 under the tag), 32:44 Lucky Shot 225 (read "22": split 5) and "Stalk…" under the tag, rec13 6:12 top row "Specimen II" not a known source (disregarded), rec16 24:53 alive between deaths, 33:23 the 32:44 panel rewritten by the game (2,929 -> 5,963) |
| `00580`-`00583` | coach-review-8 D vision (`crates/vision/tests/rejuvenator.rs`): rec8 39:11 alive with two Rejuvenator charge icons under our total, 39:17 Rejuvenator revive (grey world, "Respawn in: 1", no shop button), 39:22 alive again; rec13 opening clock 5:52 at 1920x1080 (read 6:52 on the downsampled context) |
