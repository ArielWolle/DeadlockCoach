# Browser migration validation

## Checks exercised locally

- Native baseline preserved in a separate commit on `feat/browser-coach`.
- `wasm-pack build crates/web --target web --release --out-dir ../../web/wasm --out-name coach_web`:
  existing `ocrs` / RTen compile and execute in a browser worker. SIMD build does not need
  cross-origin isolation or shared-memory threads.
- `cargo test --release --workspace`: **84 Rust tests passed**, including saved gameplay and
  minimap regression fixtures, lane policies/source integrity, independent HUD localization,
  region provenance/field binding, transport extension and asset-pack bounds.
- `cargo test --locked --release -p coach-perception --no-default-features`: **21 tests passed**
  with the bundled OCR backend disabled (matching, graph execution and the JSONL CLI).
- `cargo build --locked --release -p coach-agent -p coach-brain -p deadlock-coach`: native
  Agent, Brain and Tauri application all build on Windows after the refactor.
- `npm run test:unit`: **13 tests passed** using Node's test runner: settings/JSONL validation,
  crop geometry, conservative border detection, bounded history, overlay transforms and
  verified-asset cache recovery/offline/corruption rejection.
- `npm run build`: TypeScript, production Vite build, asset allow-list, manifest integrity and
  static publication checks. App tested under `/DeadlockCoach/`.
- Playwright: **30 tests passed** across the full live suite and affected recovery/counter groups.
  Coverage: navigation/mobile overflow/keyboard skip link, local settings,
  actual WASM snapshot replay, real OCR matching native clock/souls/inventory, mocked display
  capture pause/stop/track release, model failure visibility, cancellation/retry, recording
  export/import/delete, speech API invocation/muting, offline shell plus cached model replay,
  and axe-core scans of every route.
- Viewport-editor coverage includes frozen-source inspection, mouse resize, keyboard movement,
  touch controls, cancel/apply while retaining the share, automatic top/side border cropping
  feeding real OCR, source-size changes, screenshot imports without changing capture
  preferences, modal accessibility and small-screen layout.
- Automatic-profile coverage includes offset video in a 1920×1200 source without calibration,
  located field boxes, disabled compatibility-only crop settings and optional inspection. This
  test fixes `Date.now()` because its repeated still image cannot advance a game clock; actual
  timers, WASM inference and screen-stream pixels remain real.
- History coverage retains five source frames, verifies sampling stays paused while the producer
  continues, switches frames/overlays, exports an unreviewed sample and resumes the same share.
- Lane-coaching browser coverage exercises sourced camp advice, poke/back-off priorities and
  persistent focus/options. Native tests cover the broader policy matrix and missing evidence.
- Matching-lab coverage includes generic 4:3 capture without model/game-asset downloads,
  portable template assets/anchor search, visual small-field authoring, real clock/currency
  OCR through ordinary profile nodes, profile rejection without losing the active graph,
  persisted profiles, cloned browser events and JSONL export. See the [perception guide](../perception/README.md).
- `go run github.com/rhysd/actionlint/cmd/actionlint@v1.7.11 -color`: all workflow files parse
  and validate without findings.
- `npm audit`: dependency audit clean after removing the unnecessary vulnerable test-runner dependency.

The Playwright fixtures use the real WASM/model implementation, not a fake OCR response.
The OS screen picker and speech hardware are mocked; the automatic still-image capture also
uses a fixed frame wall clock as described above. Screenshot outputs/traces are local
test artifacts and uploaded as short-lived CI test evidence, not included in the published site.

## Performance evidence

The initial recorded-frame browser regression logged **6,176 ms** after the SIMD build,
minimap ring optimization and direct team-total line recognition. The full viewport-editor
regression run logged **11,728 ms** on its final OCR frame; stream diagnostics also ranged
up to about 15 seconds. Runtime varies with the browser and machine load. Tests attach this timing as JSON. It is not game FPS,
not an SLA, and not validation of capture while the game has foreground focus.

The final profile-engine regression run logged **5,663 ms** for the legacy Deadlock OCR
fixture's last frame. Generic pipelines process only their configured stages; the color-only
test uses no neural models. These observations do not establish a fixed live-game latency.

The automatic-HUD/lane-policy validation run logged **17,844 ms** on the saved screenshot
replay's final OCR frame under concurrent native build/test load. This reinforces that the
five-second goal is not met reliably; the new health/localization passes also consume time.

After the fast perception framework (tracked boxes, one batched recognition call per frame,
extrapolated clock, cross-origin-isolated parallel recognizer pool), the full suite (31 browser
tests, 13 unit tests) logged a replay last frame of **1,389–1,796 ms**, mostly a rare clock
verification. Steady per-frame work was a 12-line batch of 170–280 ms plus an auto batch of
66–83 ms with 4 parallel recognizers, and the fast monitor took 495–783 ms. These are headless
fixture numbers, not live-game measurements.

## Remaining acceptance work

See [Twitch stream smoke tests](stream-tests.md) for an additional real 1080p stream-frame
check: clock and souls matched the visible HUD; totals/roster/items/minimap still need wider
coverage. This check used screenshot replay and does not satisfy the live-capture gate.

See [blockers.md](blockers.md). Pages now uses GitHub Actions from `main`; hosted results are checked in GitHub,
and live OS capture/headphones/background/game performance require a real gaming session.
No whole-site WCAG conformance claim is made from automated checks and keyboard smoke tests alone.

## Clean-checkout CI regression

Run `40344319180`, job `121647058674`, failed because `samples/frames` was missing from
the checkout: `.gitignore` used `frames/`, which ignored nested fixture directories too.
The fixtures had existed only locally. The rule is now anchored to `/frames/`, and the
eight saved regression images are versioned with a fixture inventory.

Verification used `git checkout-index --all` into a new temporary source directory after
staging the fix, so ignored local files could not hide missing inputs. The exact portable
Rust CI test command passed **47 tests**, including all six full-frame tests. `npm ci` and
the asset build also passed from that clean copy. The static website's fixture allow-list
is unchanged.

## Mixed-version asset-cache regression

An existing tab reported `Asset integrity check failed for timings.json`: its service worker
used network-first manifest retrieval but cache-first unversioned small assets. After a build
update, that could pair the new manifest with old shell-cached timing bytes. Merely deleting
the analysis worker's cache entry did not bypass the stale shell response.

Verified asset requests now include the expected SHA-256 in the URL, isolating them from old
unversioned service-worker and HTTP cache entries. A hash/length mismatch deletes the bad entry
and retries once with `cache: reload`; the response must still match both checks. New service
workers forward reload requests directly and use only their own shell cache for other lookups.

The added real-WASM browser regression seeds stale unversioned shell timings and a corrupt
hash-keyed analysis-cache entry, initializes successfully, and verifies repaired cached bytes.
That regression plus the existing offline, model-failure and initialization-cancellation tests
passed (**4 targeted browser tests**). Three additional unit tests cover corruption recovery,
offline cached bytes, quota failure and persistent corruption rejection. The production build,
TypeScript and publication-integrity checks pass. The browser suite now contains 27 scenarios;
the unrelated scenarios were not repeated for this cache-only fix.

## One-second acquisition and independent fast monitoring

The subsequent live-coaching run passed all 81 workspace Rust tests, 21 no-OCR perception
tests, 13 Node tests and 28 browser tests, plus native/WASM builds and publication checks.
The fast real-WASM regression logged **543 ms** for its final quick update and **20 captures
during two full scans**. The saved full-OCR replay logged **9,643 ms** on its last frame.
Tests cover sampling during busy full inference, paused inspection, twelve critical/unknown
health slots, native death/menu/stale-geometry rejection, gank prerequisites, earned-souls
pace abstention/reinforcement, immediate health priority, aged/queued countdown expiry and
normal speech preemption. See [live coaching](../live-coaching.md) for the evidence limits.

## Full-scan recovery and deployment preparation

The recovery regression deliberately withholds a full-worker frame result while using real
WASM for both other paths. It verifies automatic reconstruction, continuing screenshots and
fast results, and exactly one screen-share permission request. Native tests cover conservative
health/clock-only output after rich context expires, plus menu/death rejection. Minimap/text
budget changes and batched scoreboard names retain the image regression results.

The recovery/automatic/fast browser group passed all 3 tests; the other general app, crop,
cache, profile, lane and item-counter group passed 26 tests. The pure generic history regression
also passed again alongside the final recovery test.
The suite now contains 30 scenarios. One loaded run measured a 995 ms fast update and an
11,385 ms full-scene replay, reinforcing that those timings must be displayed separately.

The public repository's previous feature commit `b6de20f` passed hosted CI run `40392883640`.
Pages configuration was changed from legacy root publishing to the Actions artifact workflow,
and the explicit `PAGES_ENABLED=true` repository variable was verified before promoting the app.
