# Deadlock Coach

A local-first Deadlock coach: screenshot → local OCR/vision → structured match state → rules → spoken cues.

## Flexible UI-matching engine

**Matching lab** now exposes a game-independent perception pipeline: crop/anchor → preprocess
→ OCR/template/color detector → parse → gate → confirm → named observations. Profiles are
portable JSON with embedded template assets, visual region authoring and per-stage diagnostics.
Generic profiles support arbitrary aspect ratios and run without Deadlock-specific advice;
non-OCR profiles also avoid loading neural models. Deadlock remains a specialized adapter.

The reusable `coach-perception` Rust crate runs natively and in WASM. Its `coach-perceive`
CLI accepts JSONL image paths/base64 and emits JSONL observations; the browser also publishes
`coach:perception` events and exports observations. See [profile authoring and extension points](docs/perception/README.md).

## Browser app (no application backend)

Open the app **on the gaming PC**, click **Start coaching**, share a game/video window,
and route browser audio to your headphones. The automatic Deadlock profile discovers HUD
elements without a screen-size setting or required crop. Processing stays inside a dedicated browser
worker using the same Rust vision and rule engine as the native app.

- Capture, HUD OCR, inventory/scoreboard recognition, minimap estimates and coaching.
- Active lane plans: catch/clear waves, hold, take short poke windows, use a conditional
  one-camp window, shop or prepare a rotation—with evidence and document citations.
- Browser speech, voice/category settings, mute, pause and stop controls.
- One-second screenshots and a separate fast HP/minimap worker; earlier objective reminders,
  wave reports, positive farm-pace feedback and conditional gank checks.
- Optional frozen five-frame HUD inspector with located/configured overlays and sample export.
- Optional legacy-profile black-border detection and manual viewport adjustment.
- Screenshot and JSONL replay through the actual WASM engine.
- Searchable versioned knowledge base with original sources and uncertainty labels.
- Bounded opt-in local recording with export/import/deletion.
- Installable app manifest and offline shell; models cache after the first successful initialization.

### Build locally

Prerequisites: Rust **1.94.1+**, Node **24 LTS** (22.12+ also supported), Windows/macOS/Linux.

```powershell
rustup target add wasm32-unknown-unknown
cargo install wasm-pack --locked --version 0.13.1
npm --prefix web ci
npm --prefix web run prepare:app
npm --prefix web run build
npm --prefix web run preview
```

Open **http://127.0.0.1:4173/DeadlockCoach/**. Localhost is a secure context for capture.
For existing installations, choose **Screenshot interval: 1** in Settings to replace a saved
five-second preference. See [live cadence, health and farming evidence](docs/live-coaching.md).
For frontend development use `npm --prefix web run dev` after `prepare:app`.

The WASM build uses SIMD, supported by modern desktop Chrome and Edge. It does not require
SharedArrayBuffer, COOP/COEP headers, browser extensions, API keys or a native companion.
Changing Rust code requires rebuilding WASM. Changing data/docs requires rebuilding assets.

### Tests

```powershell
cargo test --locked --release -p coach-core -p coach-engine -p coach-perception -p coach-vision -p coach-web
npm --prefix web run test:unit
npm --prefix web exec playwright install chromium
npm --prefix web test
```

Browser tests serve the **production** artifact under `/DeadlockCoach/`, not just Vite development mode.
They test real WASM inference on saved gameplay frames plus mocked display capture. They cannot
prove OS screen-picker behaviour, headphone routing, sustained game FPS, or background/fullscreen
capture stability; see [validation and blockers](docs/web-migration/blockers.md).

### GitHub Actions and hosting

CI runs on pushes/PRs and uploads `web-dist` plus browser-test evidence. Windows checks retain
native compatibility. **Pages deployment is off by default**, so a private repository with no
available Pages hosting does not make CI fail.

When hosting becomes available:
1. Enable Settings → Pages → Source: **GitHub Actions**.
2. Set repository Actions variable **`PAGES_ENABLED=true`**.
3. Merge this branch to `main`, or manually run **Publish website (opt-in)** on `main`.

Expected URL: `https://arielwolle.github.io/DeadlockCoach/`.
Alternatively, serve the `web-dist` artifact on any HTTPS static host. Set `BASE_PATH=/` at
build time for a root-hosted site. GitHub Pages and Actions serve/build files; no match is
processed on a GitHub runner or server.

## Accuracy and supported environments

The automatic Deadlock prototype independently locates the minimap, Souls text and clock in
the captured window. Health, hero, item and scoreboard readers still use compatibility layout
context registered from those elements. Small scoreboard icons/wave markers and enemy Urn
possession remain approximate. Generic profiles define their own detectors, regions and evidence
checks. Unknown is not interpreted as safe or absent; arbitrary rearranged HUDs and other games
need appropriate locator profiles and representative examples. Camp availability is not detected.
Source data is dated and meta statistics are correlational.

Screen sharing always requires an explicit browser selection. Closing the app, stopping sharing,
or sleeping the tab interrupts coaching. A PWA installation cannot bypass these rules.

### Frame your game or Twitch video

New sessions use automatic, per-element discovery. **Adjust viewport** is optional inspection
and authoring tooling; automatic profiles do not require it. See [automatic HUD discovery](docs/automatic-hud.md)
for implemented locators, provenance and current coverage. The following controls remain useful
for the explicit calibrated legacy profile:

Black borders are checked when sharing starts and when the source size changes. If a stable
16:9 content area is found, it is cropped automatically. Otherwise an incompatible viewport
opens the visual editor instead of repeatedly showing an aspect-ratio error.

Use **Adjust viewport** on Coach or **Open visual crop editor** in Settings at any time:
drag the frame to move it, drag a corner to resize it, and check the cropped preview. The
selection stays 16:9. **Auto-detect borders**, **Fit 16:9**, **Center**, arrow-key controls,
click-to-move controls, reset, cancel and apply are available. Analysis/speech pause during
editing; applying reuses the active share. Manual selection takes priority for that session.
Percentage inputs remain under **Advanced percentage controls**.

Automatic cropping is conservative: a dark/blank scene or inconsistent edges are not treated
as confirmed borders. Use the editor when detection is uncertain or overlays obscure the edges.
Screenshot imports also recover from an incompatible viewport without altering live preferences.

## Native application

The Cargo workspace retains `coach-agent` (gaming PC capture/audio), `coach-brain` (analysis),
and the Tauri desktop UI. Build on Windows with `cargo build --release --workspace`.
The browser version does not need these programs. Existing scripts and SSH-based deployment
remain available for the original remote-Brain workflow.

## Documentation

- [Migration architecture](docs/web-migration/architecture.md)
- [Game profiles and perception pipelines](docs/perception/README.md)
- [Blockers and validation](docs/web-migration/blockers.md)
- [Local validation results](docs/web-migration/validation.md)
- [Knowledge base](docs/README.md)
- [Active lane policy and evidence](docs/active-laning.md)
- [Automatic HUD discovery and five-frame inspection](docs/automatic-hud.md)
- [Minimap implementation](docs/minimap-implementation.md)
- [Published asset attribution](web/public/assets/NOTICE.txt) (generated by the asset build)

Independent personal project, not affiliated with Valve. Game artwork belongs to Valve.
Wiki-derived text retains its original CC BY-SA attribution and source URLs.
