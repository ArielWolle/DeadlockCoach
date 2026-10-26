# Profile-driven UI perception

The matching engine is now a separate **`coach-perception`** crate. It depends on images,
OCR and JSON, and has no dependency on Deadlock snapshots, coaching rules, capture or audio.
It runs natively and in the browser's WASM worker.

```text
Frame provider (screen share / screenshot / native producer)
  → profile-driven perception graph
      crop / anchor → preprocess → detector → parser → gate → temporal confirmation
  → named observations + source bounds + per-stage diagnostics
  → consumers (UI / JSONL / browser event / game-specific coaching adapter)
```

The existing Deadlock scanner is registered as `deadlock.scan`, a custom stage. Its game
interpretation and coaching behaviour are preserved. Generic profiles run independently and
do not emit Deadlock advice. Supporting another game's UI means authoring a profile and its
assets; game-specific strategy advice requires an appropriate coaching adapter.

`deadlock-hud-fields.json` also demonstrates the same game's clock/currency using ordinary
crop/OCR/parser/confirmation nodes, without the custom scanner or template pack. It is an
observation-only example for incrementally replacing specialized HUD logic with profiles.

This is a reusable recognition framework, not a pretrained universal game/HUD recognizer.
Regions, icons, parsing and evidence thresholds still need validation for the target UI.

## Browser authoring

Open **Matching lab**:

1. Load **Any game · OCR starter** or **Any game · color/bar starter** into the draft.
2. Load a reference screenshot, or use the latest captured frame.
3. Add a named field, choose its crop stage and use **Select region on reference**.
   This editor is free-aspect and supports tiny fields independently of game-viewport cropping.
4. Tune OCR, extraction, bounds, scores and confirmation in the JSON. Importing a template
   image embeds a small PNG and creates a searchable anchor stage.
5. **Apply profile** validates the complete graph/resources before replacing the current one.
   A failed import leaves the previous engine usable.
6. Run the reference frame, or start live capture. Inspect published fields and every stage's
   status, value, image dimensions, similarity score, bounds, error and execution time.
7. Export the profile, a detailed current report, or observation JSONL.

Profiles without OCR do not download neural models. Generic OCR profiles download their models
without loading the Deadlock template pack. Viewport aspect ratio and maximum processing width
belong to the profile: generic profiles can use 4:3, portrait, ultrawide or an unrestricted aspect.
The legacy Deadlock adapter retains its calibrated 16:9/1600-wide input.

Draft changes are separate from the active profile. Active profiles are stored locally when
browser storage permits; a self-contained JSON export also carries embedded template assets.
The report-history export is bounded to 300 observations / approximately 4 MB of JSON text.

## Profile contract

Examples live in `data/profiles/`. This complete no-OCR example measures a colored bar:

```json
{
  "schema_version": 1,
  "id": "my-game",
  "name": "My game HUD",
  "adapter": "generic",
  "viewport": { "aspect_ratio": null, "max_width": 1600 },
  "nodes": [
    {
      "id": "health_roi",
      "op": { "type": "crop", "region": { "x": 0.04, "y": 0.9, "w": 0.25, "h": 0.025 } }
    },
    {
      "id": "fill",
      "inputs": ["health_roi"],
      "op": { "type": "color_ratio", "rgb": [60, 200, 80], "tolerance": 45 }
    },
    {
      "id": "percentage",
      "inputs": ["fill"],
      "op": { "type": "parse", "parser": "number", "scale": 100, "min": 0, "max": 100 }
    },
    {
      "id": "health",
      "inputs": ["percentage"],
      "op": { "type": "confirm", "frames": 2, "tolerance": 5, "max_age_ms": 15000 }
    }
  ],
  "outputs": { "player.health_percent": "health" }
}
```

- Nodes form a DAG; declaration order is irrelevant. `$frame` is the implicit image input
  for image operations with no `inputs`.
- `inputs` name preceding graph results. Branches can share crops and OCR; equivalent OCR
  calls on a shared image/mode/upscale are memoized within that frame.
- `when` names a boolean-producing stage. False/missing conditions skip the branch and
  clear its temporal confirmation. Conditional dependencies participate in cycle detection.
- `outputs` map arbitrary field names to stage IDs. These names are the consumer-facing API.
- Imported profiles cannot execute JavaScript. Custom stages must already be registered by
  the host. Resource names, templates, references, cycles and limits are checked before activation.

### Operations

| Operation | Parameters / purpose |
| --- | --- |
| `crop` | `region`, optional `anchor_input`; produces an image packet with original-frame bounds |
| `preprocess` | `grayscale`, `invert`, `threshold`, `scale`; coordinates remain tied to the original analyzed frame |
| `ocr` | `mode: detect|line`, `upscale: 1..6`; line mode skips text-layout detection |
| `extract` | Rust-regex `pattern`, `group` (default 1); extracts a capture from text |
| `parse` | `parser: text|number|clock`, optional `min`, `max`, `scale`, `decimal_comma` |
| `contains` | `text`, optional `case_sensitive`; boolean scene/label test |
| `select` | JSON-pointer `path` into a structured detector result |
| `gate` | `min_score` and/or exact JSON `equals`; withholds results that do not qualify |
| `confirm` | `frames`, numeric `tolerance`, `max_age_ms`, optional `max_delta` |
| `status` | `one_of: [missing,error,...]`; inspects an input status, including missing/error inputs |
| `coalesce` | First available value from two or more input branches |
| `match` | Embedded `templates`, `min_score`, `min_margin`, `color` (default true); icon classification |
| `locate` | Embedded `template`, `scales`, `step`, `min_score`; searches for an anchor and publishes its box |
| `classify` | Host-registered color-template `library`, `min_score`, `min_margin` |
| `color_ratio` | `rgb`, channel `tolerance`; fraction of matching pixels in an image |
| `custom` | Registered stage `name`, arbitrary JSON `config` |

Number parsing handles grouping separators and k/m suffixes, with explicit decimal-comma
mode. Clock parsing requires a separator and supports MM:SS / HH:MM:SS. It does not guess a
colon from an arbitrary digit string. Use extraction plus numeric bounds for specialized UIs.

#### Lazy fallback

To avoid running an expensive OCR detector whenever a quick line read succeeds:

```json
[
  { "id": "fast", "inputs": ["roi"], "op": { "type": "ocr", "mode": "line", "upscale": 1 } },
  { "id": "primary", "inputs": ["fast"], "op": { "type": "parse", "parser": "clock" } },
  { "id": "retry", "inputs": ["primary"], "op": { "type": "status", "one_of": ["missing", "error"] } },
  { "id": "slow", "inputs": ["roi"], "when": "retry", "op": { "type": "ocr", "mode": "detect", "upscale": 3 } },
  { "id": "secondary", "inputs": ["slow"], "op": { "type": "parse", "parser": "clock" } },
  { "id": "clock", "inputs": ["primary", "secondary"], "op": { "type": "coalesce" } }
]
```

### Coordinates and anchors

Crop regions support:
- `space: normalized` (default): fractions of their input image.
- `space: pixels`: processed-image pixels.
- `space: reference`: values scaled from `reference_width` / `reference_height` (defaults 1600×900).
- `anchor: top_left|top_right|bottom_left|bottom_right|center`.
  Corner offsets are inset from that corner. Center offsets move the rectangle's center.

`locate` publishes both a matched image and its original-frame bounds. A crop with
`inputs: ["$frame", "anchor_stage"]` and `anchor_input: true` measures its region relative
to that box, allowing a text field next to a recognized icon. Pixel offsets can extend
outside the anchor box, provided the resulting crop remains inside the frame.

Regions after preprocessing still report bounds in the original **analyzed frame**, not
resized intermediate images. `frame_size` identifies that coordinate plane. Mapping to raw
desktop coordinates additionally requires the capture viewport and downscale transform.

### Templates and matching limits

Portable templates use `templates: [{"id":"icon","png_base64":"..."}]`. The browser helper
converts images to PNG and caps them at 256 pixels per side. The native decoder also applies
byte/dimension limits. Templates are measured in processed-frame pixels; configure `scales`
when their source size differs from the target UI.

Icon matching supports RGB descriptors and grayscale NCC; color helps distinguish otherwise
similar icons. Anchor search supports multiple scales, pixel-precision exhaustive search for
small regions and coarse-to-fine refinement for larger ones. It has a bounded candidate budget.
Crop the search area or use a coarser step when a stage reports a budget error. This is
template matching, so animation, rotation, occlusion, theme changes and very small icons
still require appropriate assets, strategies or another detector backend.

### Evidence semantics

Statuses are `ok`, `missing`, `pending`, `skipped`, `error`. Only `ok` readings publish a value.
An image-only packet can publish bounds without a text/numeric value. Missing/skipped inputs
do not reuse old confirmed values. Confirmation expires after gaps/backwards timestamps and
resets on changed values outside tolerance. `max_delta` requires renewed confirmation after
a large observed jump.

Scores are detector similarity, **not calibrated probabilities**. The bundled OCR backend
does not supply a recognition score, so it stays null. A score gate cannot pass an unknown
score. Repeated agreement can still be consistently wrong OCR; validate on representative
frames. `ok` means the configured evidence checks passed, not that the value is guaranteed true.

## Native JSONL interface

```powershell
cargo build --release -p coach-perception --bin coach-perceive
'{"image":"samples/frames/00059_1792966931596.jpg","timestamp_ms":5000}' | .\target\release\coach-perceive.exe --profile data/profiles/generic-hud.json --models data/models
```

For a continuous producer:

```text
frame-producer | coach-perceive --profile my-game.profile.json --models data/models | observation-consumer
```

Each stdin line contains exactly one of `image` (local file path) or `image_base64` (PNG/JPEG
bytes, standard base64), plus producer-supplied `timestamp_ms`. Each stdout line is one
report and is flushed immediately. Errors/logs use stderr; malformed input/configuration
exits nonzero. Stage-level failures remain inspectable inside a report and may feed a fallback.
`--models` is unnecessary for profiles using only templates, colors and other non-OCR stages.
The crate's default `ocrs-backend` feature is optional. Build with `--no-default-features`
for a color/template-only binary, or depend on the core without that feature and register
your own `TextBackend`. The graph, packet and observation contracts remain the same.

The standalone CLI hosts generic stages. Applications register additional custom stages
through `Resources`; the browser host additionally registers the Deadlock adapter.

## Observation contract and browser pipe

Reports include `schema_version`, `profile_id`, a canonical SHA-256 `profile_revision`,
frame-local `frame_id`, producer `timestamp_ms`, `frame_size`, named `fields` and `stages`.
Frame counters restart when a pipeline resets; they are not global session identifiers.
Full reports include per-stage diagnostics; browser history JSONL exports the compact fields.
Raw pixels are not serialized into reports.

```js
window.addEventListener('coach:perception', event => {
  const report = event.detail;
  const health = report.fields['player.health_percent'];
  if (health?.status === 'ok') {
    // Feed another local visualization, recorder or application adapter.
    console.log(report.profile_id, report.profile_revision, health.value);
  }
});
```

The event carries a clone of the report. No network sink is enabled by default. Recordings
can include the perception report and profile ID alongside their image/snapshot metadata.

## Native extension points

- `TextBackend`: replace OCR while keeping crop/parser/gating stages. Return a score only
  when your backend actually provides one.
- `Stage`: implement a detector, specialized parser or whole game adapter. `run` accepts
  upstream packets, frame context and JSON config; `reset` clears per-session state.
  Registered stage instances are shared by name; `frame.node_id` lets a stateful implementation
  distinguish multiple uses of that stage in the graph. A runtime is owned by its processing
  worker; published reports can be passed to other threads/processes independently.
- `Resources.libraries`: register shared color-template libraries.
- `Pipeline::set_profile`: validates/compiles atomically. Rejected graphs keep the previous
  profile; changing a valid profile resets temporal/custom-stage state.

The browser implements the Deadlock scanner as a `Stage`, then routes its snapshot to the
existing coaching engine. New generic fields remain independent of that snapshot schema.

## Verification

Native tests cover DAG order/cycles, atomic changes, crop-coordinate transforms, explicit
fallbacks, lazy status-gated inference, scene changes/expiry, replaceable/cached OCR backends,
unknown scores, locale parsing, color disambiguation, anchor search and adjacent regions,
profile revisions, plus JSONL file/base64 input. Browser tests cover 4:3 live capture without
game assets/models, real OCR, visual region authoring, template assets/search, invalid graphs,
profile persistence, browser events and JSONL export. Existing Deadlock regressions remain.
