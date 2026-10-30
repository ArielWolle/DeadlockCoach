# UI simplification (wt/ui)

The goals: say each thing once and in the right place, keep implementation wording out of the
player's view, drop the Matching lab, and cut clutter. The coaching behaviour is unchanged.

Screenshots are 1280 px wide. They show the empty state and the state after
**Load included match**. `before-*` is the old UI and `after-*` is the new one.

| Page | Before | After |
|---|---|---|
| Coach | [before-coach](before-coach.png) · [replayed](before-coach-replayed.png) | [after-coach](after-coach.png) · [replayed](after-coach-replayed.png) · [mobile](after-coach-mobile.png) |
| Map intelligence + Match state → **Live match** | [before-map](before-map.png) · [before-match](before-match.png) · [replayed](before-match-replayed.png) | [after-match](after-match.png) · [replayed](after-match-replayed.png) |
| Player | [before-player](before-player.png) | [after-player](after-player.png) |
| Replay lab | [before-replay](before-replay.png) | [after-replay](after-replay.png) |
| Matching lab | [before-pipeline](before-pipeline.png) | removed |
| Field guide | [before-knowledge](before-knowledge.png) | [after-knowledge](after-knowledge.png) |
| Settings | [before-settings](before-settings.png) | [after-settings](after-settings.png) |

## Inventory: what was said more than once

| Message | Where it appeared before | Where it is now |
|---|---|---|
| "Analysis is local; the AI coach sends text only" | Sidebar ("LOCAL-FIRST ANALYSIS" plus two lines), Coach intro ("Powered by local Rust & WebAssembly…"), Settings intro, AI settings, Player intro, the "Only you can see this feed" line in the feed | Said in full once, next to the AI option in Settings. The Coach intro has one short line: "Screenshots never leave your browser." |
| Session status | Top-bar engine tag (ENGINE OFFLINE / LOCAL ENGINE READY), session line plus source line, feed badge (AWAITING CAPTURE / LIVE · LOCAL ANALYSIS / REPLAY · SILENT), and notices | One status in the top bar (dot, state and source), visible on every page. The engine state moved to Settings › Diagnostics. The feed badge is gone. |
| Taglines | "YOUR EDGE, IN REAL TIME", "THE CURIOSITY SHOP, FOR YOUR DECISIONS.", "LESS GUESSWORK. BETTER DECISIONS.", the WATCH/THINK/ACT stamp, an eyebrow on every page, the footer's "A quieter kind of advantage." | Removed |
| Waiting copy on the Coach page | "WAITING FOR EVIDENCE" / "WAITING FOR GAMEPLAY", "Share gameplay to build a lane plan.", "The coach prioritizes safety…", "HP unconfirmed · Player lane unconfirmed", "Advice arrives when…", "No calls yet. Good advice is worth waiting for.", "VOICE READY" | One short line per panel: "Start coaching to hear your first call.", "Waiting for gameplay.", "No calls yet." Unread health and lane are left out, not listed as "unconfirmed". |
| Allied and enemy health | The Coach page's "Live health & farming pace" panel (text list), plus the Match state roster | Only the Live match **Heroes** roster. The farming pace line moved to Live match › Objectives. "Your hero" now sits next to "Lane focus". |
| Why a lane call was made | Advice context ("HIGH · 18:27 · <rationale>") and Next best move | Only under Next best move. The advice context is now "18:27 · HIGH". |
| Minimap evidence | The Coach "MINIMAP READ 3/3 reads" card and the Map page "N RECENT OBSERVATIONS" tag | Only the Live match minimap tag. |
| Tech labels in metrics | "Temporally checked OCR", "Confirmed across frames" | Removed |
| Analysis timings | Feed footer ("16:9 viewport · approx. 5-second sampling", "Fast HP/map…", "Full scene: … ms"), Live health panel ("Fast monitor waiting for located HUD") | Settings › Diagnostics |
| Recording storage rules | A long paragraph in Settings, plus another in the Replay lab shelf | One line in each place: what the setting does, and how storage behaves |

## What changed

**Navigation:** Coach · Live match · Player · Replay lab · Field guide · Settings. The old
`#map` link opens Live match. `#pipeline` falls back to Coach.

**Coach:** a short heading, then one primary action (**Start coaching**), Pause, Stop and a quiet
**Adjust viewport**. Below that come the live advice (the `aria-live` region is unchanged) next
to the game feed, then three match numbers (clock, unspent souls, team net worth), then
**Next best move** with its two focus controls (Lane focus, Your hero). Pep talk and AI coach sit
side by side. While the AI coach is off, its panel is one line and its buttons are hidden. The
pep-talk buttons are hidden until there is a pep talk to replay or stop. Recent calls come last.
The "Profile readings" panel and the footnote linking to the Matching lab are gone.

**Live match:** the Map intelligence and Match state pages merged into one page, in this order:
minimap, lane fronts, objectives with farming pace, heroes, your items and enemy items, and a
collapsed "Raw snapshot (for bug reports)".

**Settings** has these sections:
- Voice
- What the coach talks about (lane coaching is now in the HTML, not inserted by script)
- Recording
- AI coach (optional), with rarely used fields under **More AI options**
- **Advanced**, collapsed: screenshot interval, game area (viewport editor; the percentage
  controls appear only for a calibrated, non-automatic profile), Restart analysis, Clear
  downloaded data
- **Diagnostics (for bug reports)**, collapsed: analysis state, detection profile, runtime
  table, timings

Technical names such as WASM or cross-origin isolation now appear only in Diagnostics.

**Matching lab removed:** this covers `profile-panel.ts`, its page and CSS, the profile JSON
editor, starter profiles, field and template authoring, reference-frame runs, and the stage
inspector. In `main.ts` that removed `loadReferenceImage`, `pickFieldRegion`, `runReference`,
`profilePanel` and the generic "Profile readings" panel. These parts stay:
- `applyGameProfile` and the stored-profile mechanism (`profiles.ts` now bundles only the
  Deadlock automatic profile). A non-default stored profile is named in Diagnostics with a
  **Use default detection** button, so anyone who applied a Matching lab draft can get back. The
  default profile is no longer written to storage.
- The `isDeadlock` gates, unchanged.
- The viewport editor and "Adjust viewport" recovery. They are reachable from the Coach
  toolbar, the notice recovery button and Settings › Advanced.

**Other copy:** status and notice wording is plainer ("Ready", "Loading the coach…",
"Recovering", "Restart analysis in Settings › Advanced"). The `#engine-state` values are now
Not loaded / Loading / Ready / Error / Recovering. Notice auto-dismiss behaviour is unchanged.

**Tests:**
- `app.spec.ts`: the route list and accessibility pass are updated, and an axe pass now runs
  with the collapsed Settings sections open. A new test checks the merged page, the
  navigation, and that the Coach page has no implementation wording.
- `profiles.spec.ts`: the Matching lab UI tests are removed. It keeps a stored generic-profile
  live test and adds a test for **Use default detection**.
- `ai-coach.spec.ts` opens **More AI options**.
- The engine-state and drop-zone label assertions are updated.

## Unchanged on purpose

These were left alone: player-page rendering (including the matches table, which a parallel
branch is editing), the AI review, pep talk and AI coach logic, coaching and cue delivery, the
crop-editor internals, and the `data/profiles/*.json` files that the Rust tests use.
