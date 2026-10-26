# Migration blockers and validation

## Resolved during implementation

| Issue | Resolution |
| --- | --- |
| Existing vision construction required filesystem paths | Added in-memory OCR/template/scanner constructors, retaining native adapters |
| No WASM target/build tooling installed locally | Installed wasm32 target and pinned wasm-pack 0.13.1; native `ocrs` 0.13 / RTen 0.26 compile for browser |
| Pages cannot provide arbitrary isolation headers | Single-thread WASM worker; no SharedArrayBuffer requirement. Real browser initialization tested with `crossOriginIsolated=false` |
| Browser workers cannot execute piper.exe | Built-in speech API; prefer local installed voice, with explicit test button |
| Screenshot compression loses minimap detail | Transfer RGBA from native capture-resolution minimap before downscaling the HUD |
| Site could accidentally publish recordings/desktop files | Build-time asset allow-list and production artifact checks; never deploy repository root |
| Browser test dependency advisories in initial candidate | Replaced the optional test-runner dependency with Node's built-in test runner; lockfile audit clean at implementation time |
| Offline module scripts missed their cached response | Static preview responses include `Vary: Origin`; same-origin static cache matches now ignore that header. Offline reload and real WASM replay pass |
| Minimap ring detector repeatedly computed trigonometry inside pixel loops | Precomputed circle offsets and dilated mask once, preserving minimap regression results and reducing detector cost |
| Repeated text detection on tiny single-line HUD fields | Added direct line recognition for calibrated team totals. Clock/name experiments degraded reads, so those retained full detection. Native and browser regressions pass |
| Letterboxed captures produced a repeated 16:9 error with percentage-only recovery | Added live visual viewport editing, typed error recovery, conservative automatic four-edge border detection and a manual override |
| Reusable UI matching was coupled to Deadlock interpretation | Extracted `coach-perception`, profile DAGs, replaceable OCR/custom stages, a JSONL CLI and browser Matching lab. The old game scanner remains a registered adapter |

## External blockers / not claimed complete

1. **GitHub Pages availability**: per owner, private repository is not currently hosting a Pages
   site. Deployment stays explicitly gated off; CI and downloadable `web-dist` are independent.
   No hosting settings, repository visibility or paid plan are changed automatically.
2. **Actual game capture while foregrounded/fullscreen**: automated tests inject a canvas-based
   MediaStream with real saved game pixels. They do not interact with an OS share picker or
   prove capture continuity during a match. Needs a live 20–30 minute Chrome/Edge game session.
3. **Headphone output and voice quality**: browser speech methods can be tested in code; audible
   output, voice installation and OS output routing need a human check on the gaming PC.
4. **Performance target not fully met**: after enabling SIMD, optimizing minimap rings and
   using direct line recognition for team totals, recorded browser OCR frames have taken
   roughly **6–15 seconds** on this machine. The latest full editor-regression run logged
   **11,728 ms** for its final OCR frame; the later profile-engine regression recorded
   **5,663 ms** (recorded 1600×900 HUD, not a live game benchmark).
   This exceeds the default five-second target.
   The loop skips queueing and samples the current frame when ready; late speech is discarded.
   A 10-second interval is a practical setting to try. Game FPS impact, thermal load and
   background scheduling remain unmeasured. A guaranteed five-second cycle requires more work.
5. **Vision uncertainty retained from native**: arbitrary HUD scales/aspect ratios, tiny scoreboard
   icons, exact K/D/A, camp availability and enemy Urn ownership remain incomplete or heuristic.
   Migration is not a claim that these recognition problems are solved.
   In particular, inherited `data/timings.json` models Hard camps as a recurring clock
   event although actual camp respawns depend on clearing; its reminder is not evidence
   of an available camp. That game-data rule needs a separate correction/verification.
   Stream diagnostics also showed a 1080p clock digit misread after preprocessing (33:29
   read as 33:20); auto-cropping does not establish exact OCR correctness on every source.
6. **Hosted Actions run status**: workflow syntax/expressions are validated with actionlint,
   and their build/test commands are exercised locally. This environment has git but no `gh`
   CLI (the owner said not to install it), so hosted private-repository checks cannot be inspected
   here. After push, use the repository Actions tab to verify runner execution. This is distinct
   from the Pages activation blocker.

## Validation procedure

Run the documented Rust, production build, unit and Playwright commands. Browser regressions
must cover real model initialization, native parity on clock/souls/item detection, capture
pause/stop/track release, settings persistence, error visibility, static-subpath hosting,
offline reload/replay and automated accessibility on every route.

For final real-game acceptance, use Start coaching on the gaming PC, select the game window,
test the voice, play with the browser backgrounded, resize/restore the game, stop sharing,
restart it, and monitor inference times. Record observations here rather than checking off
the live-game gate based on mocked capture tests.
