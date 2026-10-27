# Browser migration architecture

## Same-PC mode

The browser on the gaming PC captures the selected game window, samples its latest video
frame, and extracts RGBA buffers for the HUD and native-resolution minimap before scaling
the main viewport to 1600×900. A transferable-buffer message crosses to a dedicated worker.
`coach-web` calls the shared `coach-vision` Scanner and `coach-engine` rules. Structured
snapshots and cues return to the UI; speech runs on the main thread using the OS/browser voices.

No screenshots are sent to an application backend. Only static app/asset requests are needed.
Users may choose non-local installed voices; these may use the browser/vendor's speech service.

## Shared Rust

Reusable matching, OCR and graph execution now live in `coach-perception`, independently of
Deadlock state. `coach-vision` retains Deadlock interpretation and re-exports the matching/OCR
APIs for native compatibility. The browser registers `deadlock.scan` as a custom graph stage;
generic profiles use the same runtime without the game's assets, snapshot schema or advice.
See [profile authoring and extension points](../perception/README.md).

- `Reader::from_bytes` loads the same RTen detection/recognition models as native builds.
- `Reader::read_line` avoids full text detection for tightly calibrated team-total fields;
  clock, hero names and multiline regions retain detector-backed OCR for accuracy.
- `ColorLibrary::from_images` creates templates from the explicit binary asset pack.
- `Scanner::from_resources` separates construction from native filesystem loading.
- `Scanner::scan_image` accepts pixels directly; the native JPEG/PNG wrapper calls it too.
- `Coach` (WASM) validates input buffers, manages rules/options and resets replay/match state.
- `wasm32-unknown-unknown` uses SIMD but not shared-memory threads, allowing plain static hosting.

The desktop adapters remain intact; this does not embed a desktop Rust server into a web page.

## Viewport calibration and black borders

`crop.ts` keeps source-pixel geometry separate from display CSS pixels and normalized saved
coordinates. The optional inspector displays one of up to five frozen full-source frames,
reported element boxes and an editable selection. Application acquisition/analysis dispatch
pause during inspection; existing share permission is retained. Pointer capture supports mouse/touch handles;
keyboard arrows and explicit move/size buttons provide non-drag alternatives. Focus stays in
the native modal and Escape cancels. Source resizing updates the preview and selection.

`borders.ts` scans contiguous dark rows/columns from the four edges, allows sparse player
controls in bars, requires visible interior content and a plausible 16:9 result, and checks
agreement across three samples. Blank/dark frames and unstable candidates are rejected.
Automatic cropping runs at sharing start and source-size changes unless a manual edit has
overridden that session. An explicit Auto-detect action is also available in the editor.
This is heuristic border detection, not universal game-window/HUD localization.

The automatic HUD profile scans the complete source without a user viewport or aspect-ratio
requirement; missing locator evidence does not open a calibration wizard. Its inspection crop
does not change live input. Explicit legacy profiles retain viewport editing. Screenshot
calibration remains local to that import. See [automatic HUD discovery](../automatic-hud.md).

## Scheduling and cancellation

Screenshot acquisition runs independently, with a one-second default. Each of the full and
recognition-only fast workers allows one job at a time with no queued frames. Full scans skip
busy frames; the fast worker refreshes HP, map and top-bar health from recent registered context.
See [live coaching](../live-coaching.md) for measured latency and evidence limits.
Stop releases capture tracks and cancels speech;
in-flight WASM is terminated when switching modes rather than allowed to leak stale results.
Generation ids reject stale results. Worker timeouts pause coaching and expose a restart path.
Late results may update the inspector but their speech is discarded. Capture stream freshness
and lack of results are monitored; workers/PWA do not guarantee background execution.

## Assets

`web/scripts/assets.mjs` allow-lists models, three icon sets, game JSON, cited Markdown and
explicitly checked-in sample fixtures. It produces a deterministic versioned manifest with
SHA-256 hashes and an asset pack. The worker verifies hashes, caches by version and retries
via a fresh initialization after errors. `check-dist.mjs` checks artifact integrity and forbidden
desktop/credential paths. Only `web/dist` is uploaded.

The service worker caches the production shell, WASM, guide and sample replay. The model cache
belongs to the analysis worker. Offline initialization works after a successful online run;
first use still requires downloading assets. Updates wait for old app tabs to close.

## Local recording and replay

Recording is off by default. IndexedDB keeps at most 600 analyzed frames (about ten minutes) or 200 MB of image
data, trimming the oldest first. Export snapshots as JSONL or frames+snapshots as a versioned
JSON archive. Browser storage is not guaranteed durable. Imports are bounded and rendered as
text; recording archives never fetch arbitrary remote image URLs. JSONL uses the same Rust
rules; screenshot imports perform real vision, not fixture-specific mocked detection.

## Deployment

Reusable `web-build.yml`: portable Rust tests, WASM build, frontend type/unit checks,
production artifact, Playwright browser/OCR/capture/offline/axe checks, artifact uploads.
`ci.yml`: reusable browser job plus Windows native workspace checks.
`pages.yml`: deployment skipped unless `PAGES_ENABLED` is explicitly true and ref is `main`;
then reuse the same verified web build and official Pages actions.

## Frontend

TypeScript + Vite, semantic HTML and CSS; no frontend application runtime framework required.
Hash navigation supports static hosting without server rewrites. Coach, Map intelligence,
Match state, Replay lab, Field guide and Settings share the same local state. No API keys,
accounts or telemetry. Keyboard focus and reduced-motion support are included; automated
accessibility tests are not a full WCAG conformance claim.
