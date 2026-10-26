# Browser migration validation

## Checks exercised locally

- Native baseline preserved in a separate commit on `feat/browser-coach`.
- `wasm-pack build crates/web --target web --release --out-dir ../../web/wasm --out-name coach_web`:
  existing `ocrs` / RTen compile and execute in a browser worker. SIMD build does not need
  cross-origin isolation or shared-memory threads.
- `cargo test --locked --release --workspace`: 34 Rust tests, including saved gameplay and
  minimap regression fixtures, coaching policies, transport extension and asset-pack bounds.
- `cargo build --locked --release -p coach-agent -p coach-brain -p deadlock-coach`: native
  Agent, Brain and Tauri application all build on Windows after the refactor.
- `npm run test:unit`: **5 tests passed** using Node's test runner: settings/JSONL validation,
  bounded 16:9 crop geometry and conservative automatic border detection.
- `npm run build`: TypeScript, production Vite build, asset allow-list, manifest integrity and
  static publication checks. App tested under `/DeadlockCoach/`.
- Playwright: **16 tests passed**. Coverage: navigation/mobile overflow/keyboard skip link, local settings,
  actual WASM snapshot replay, real OCR matching native clock/souls/inventory, mocked display
  capture pause/stop/track release, model failure visibility, cancellation/retry, recording
  export/import/delete, speech API invocation/muting, offline shell plus cached model replay,
  and axe-core scans of every route.
- Viewport-editor coverage includes live preview refresh, mouse resize, keyboard movement,
  touch controls, cancel/apply while retaining the share, automatic top/side border cropping
  feeding real OCR, source-size changes, screenshot imports without changing capture
  preferences, modal accessibility and small-screen layout.
- `go run github.com/rhysd/actionlint/cmd/actionlint@v1.7.11 -color`: all workflow files parse
  and validate without findings.
- `npm audit`: dependency audit clean after removing the unnecessary vulnerable test-runner dependency.

The Playwright fixtures use the real WASM/model implementation, not a fake OCR response.
Only the OS screen picker and speech hardware are mocked. Screenshot outputs/traces are local
test artifacts and uploaded as short-lived CI test evidence, not included in the published site.

## Performance evidence

The initial recorded-frame browser regression logged **6,176 ms** after the SIMD build,
minimap ring optimization and direct team-total line recognition. The full viewport-editor
regression run logged **11,728 ms** on its final OCR frame; stream diagnostics also ranged
up to about 15 seconds. Runtime varies with the browser and machine load. Tests attach this timing as JSON. It is not game FPS,
not an SLA, and not validation of capture while the game has foreground focus.

## Remaining acceptance work

See [Twitch stream smoke tests](stream-tests.md) for an additional real 1080p stream-frame
check: clock and souls matched the visible HUD; totals/roster/items/minimap still need wider
coverage. This check used screenshot replay and does not satisfy the live-capture gate.

See [blockers.md](blockers.md). Pages is gated off, hosted CI status needs verification in GitHub,
and live OS capture/headphones/background/game performance require a real gaming session.
No whole-site WCAG conformance claim is made from automated checks and keyboard smoke tests alone.
