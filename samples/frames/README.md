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
