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
| GitHub Actions clean checkout lacked full-frame test images | The unanchored `frames/` ignore rule also excluded `samples/frames`. Anchored it to `/frames/`, added the eight fixtures to Git, and verified tests/asset generation from a tracked-files-only copy |
| Strategy material was largely reference-only | Added document-quoted lane policy cards, health/player-lane evidence, prioritized wave/poke/camp decisions and source explanations in the Coach UI |
| Camp/boost reminders assumed fixed recurring match-clock availability | Removed the recurring camp timer and made boost unlock a one-shot event; camp advice now explicitly requires checking whether the camp is up |
| Full-window capture required users to align the game viewport | Added an automatic default profile with independent minimap, Souls and clock locators; moved/resized video fixtures and independently moved HUD widgets are covered |
| Viewport adjustment kept consuming live source frames | Added bounded five-frame frozen history, report-driven located/configured overlays, local sample export and same-share resume |
| Fresh manifest could be paired with stale shell-cached timings | Verified asset URLs include their expected SHA-256, corrupt entries get one checked network refetch, and new service workers honor reload requests and restrict lookups to their own shell cache |
| Slow full scans delayed health, waves and last-second timing cues | Independent one-second acquisition and recognition-only HP/map worker; early timing windows, age-aware speech expiry and immediate health priority. Headless fast monitoring measured about 0.5–0.8 seconds, not a live-game benchmark |
| A stalled full scan paused the entire live session | Replace only the full worker with two bounded retries while retaining the share and screenshot loop; fast output stays freshness-gated. A real-WASM test deliberately drops a full result and verifies recovery |
| Full-window and scoreboard work had expensive repeated searches | 1920-wide automatic browser cap, bounded minimap/text proposals, grayscale reuse and batched scoreboard-name OCR with saved-frame parity checks |

## External blockers / not claimed complete

1. **GitHub Pages delivery**: the owner made the repository public and requested publication.
   Pages now uses GitHub Actions with `PAGES_ENABLED=true`; the browser application is deployed
   from `main`. Deployment still requires a successful hosted build and artifact publication.
2. **Actual game capture while foregrounded/fullscreen**: automated tests inject a canvas-based
   MediaStream with real saved game pixels. They do not interact with an OS share picker or
   prove capture continuity during a match. Needs a live 20–30 minute Chrome/Edge game session.
3. **Headphone output and voice quality**: browser speech methods can be tested in code; audible
   output, voice installation and OS output routing need a human check on the gaming PC.
4. **Performance target not fully met**: after enabling SIMD, optimizing minimap rings and
   using direct line recognition for team totals, recorded browser OCR frames have taken
    roughly **6–18 seconds** on this machine. The earlier full editor-regression run logged
   **11,728 ms** for its final OCR frame; the later profile-engine regression recorded
    **5,663 ms** (recorded 1600×900 HUD, not a live game benchmark).
    The automatic-HUD/lane-policy run logged **17,844 ms** under concurrent native build load.
    The fast perception framework since reduced the replay's last full frame to about
    **1.4–1.8 s**. Steady state is a single batched recognition call, about 0.2–0.35 s with the
    parallel pool. First-frame acquisition and rare detector verifications still take seconds in
    WASM, and sessions without cross-origin isolation are about 2× slower.
    This is the full-scene path, not the independent one-second screenshot sampler. The fast
    worker has measured about 0.5–1 second in headless fixtures, but game FPS impact, thermal
    load and background scheduling remain unmeasured. There is no guaranteed one-second full
    OCR cycle. See [live monitoring and recovery](../live-coaching.md).
5. **Vision uncertainty retained from native**: arbitrary HUD scales/aspect ratios, tiny scoreboard
   icons, exact K/D/A, camp availability and enemy Urn ownership remain incomplete or heuristic.
   Migration is not a claim that these recognition problems are solved.
    Personal HP and a green-ring player/lane estimate now have saved-frame coverage, but
    other HUD scales, portrait colors, overlays and live movement still need wider validation.
    Automatic full-window localization has narrow fixture coverage, not broad cross-game
    evaluation. Health/hero/item/scoreboard searches still use registered compatibility context;
    missing or rearranged widgets can remain unknown. See [automatic HUD discovery](../automatic-hud.md).
    All twelve hero slots now expose critical/unknown status and optional matched positions;
    numeric enemy/friendly HP fractions remain unknown. Specific ganks require confirmed
    identity/position/support evidence. Personal farming comparisons need own-hero identity
    and stable earned-net-worth readings; unspent balance is never a substitute.
   Stream diagnostics also showed a 1080p clock digit misread after preprocessing (33:29
   read as 33:20); auto-cropping does not establish exact OCR correctness on every source.
6. **Hosted Actions inspection**: this environment has git but no `gh` CLI (the owner said
   not to install it). The signed-in browser can inspect private job logs when its relay is
   connected; local command results alone are not confirmation of hosted runner success.
   This is distinct from the Pages activation blocker.

## Validation procedure

Run the documented Rust, production build, unit and Playwright commands. Browser regressions
must cover real model initialization, native parity on clock/souls/item detection, capture
pause/stop/track release, settings persistence, error visibility, static-subpath hosting,
offline reload/replay and automated accessibility on every route.

For final real-game acceptance, use Start coaching on the gaming PC, select the game window,
test the voice, play with the browser backgrounded, resize/restore the game, stop sharing,
restart it, and monitor inference times. Record observations here rather than checking off
the live-game gate based on mocked capture tests.
