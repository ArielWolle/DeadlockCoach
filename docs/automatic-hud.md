# Locator-first HUD discovery

## Normal user flow

Choose the game's automatic profile and share a screen/video window. There is no screen-size
setting and no required viewport selection. The default for new sessions is **Deadlock ·
automatic HUD discovery** (`deadlock-auto`). It processes the entire captured source with an
unrestricted aspect ratio. Existing explicitly saved profiles are preserved; **Restore Deadlock**
loads the new automatic default. The calibrated legacy profile remains available for compatibility.

The inspector is optional developer/debugging tooling, not a prerequisite for detection.

## Independent element locators

Discovery, transcription and game interpretation are separate layers:

```text
Any captured window
  → element locators (each returns its own measured box)
  → readers / value validation / temporal tracking
  → named observations with boxes and provenance
  → game-state interpretation
  → evidence-gated coaching
```

The current Deadlock prototype has:

- **Minimap:** full-frame, multi-scale masked-template proposals plus separate enemy/own-base
  landmark checks. A verified previous box can be tracked; loss triggers reacquisition. Position
  is not assumed to be the screen's bottom-right corner.
- **Souls:** full-frame mint-text proposals, OCR verification of the Souls label and number,
  and a tight detected text box. Moving that widget independently of the map is covered by a test.
- **Clock:** independently located time-shaped text, with layout hints used only to prioritize
  search. Transcription context scales from the located glyphs. Search hints are not published
  as detected boxes. Small clock digits can still be misread.
- **Health numbers:** OCR word/line boxes are exposed for the current/max-health numbers.
  Their proposals currently use automatically registered compatibility context; they are not
  yet unrestricted whole-frame health-gauge detectors.

The existing hero/item/scoreboard interpretation still uses compatibility readers within an
automatically registered game-area estimate. This preserves working behaviour while dedicated
element locators are added. Compatibility search boxes are explicitly marked **configured**.
They must not be relabelled as detected or used as evidence that every rearranged HUD is supported.

Discovered fields can be published when game interpretation is unknown, but unknown/menu/shop
states are gated out of strategy speech. A missing element stays missing; the automatic path
does not respond by asking the user to calibrate the screen. Very small, occluded or unfamiliar
UI may contain insufficient evidence. Reliable new HUD support requires representative examples
and a locator profile/backend, not a promise that architecture alone recognizes everything.

## Open output contract

`coach-perception::RegionObservation` is game-independent:

```json
{
  "id": "currency",
  "label": "Currency text",
  "bounds": { "x": 255, "y": 832, "w": 114, "h": 31 },
  "origin": "detected",
  "score": null,
  "value_path": "/balance"
}
```

- Bounds use the analyzed frame's pixel coordinates; `Report.frame_size` identifies that plane.
- `origin` is `configured` or `detected`. Located geometry and a transcribed value are distinct
  evidence; neither a guessed search region nor an OCR score absent from the backend is fabricated.
- `value_path` optionally binds a box to a JSON pointer in a detector's structured output.
  A downstream `select` propagates that box/provenance with the named field value.
- Packets/traces expose region collections; named readings expose `bounds_origin` too.
- `locate_text` selects an OCR-detected line by regex and returns the actual located image/box.
  It can feed parsers or an anchor-relative crop without fixed screen coordinates.
- Template, OCR, geometric or learned detectors can publish the same contract. The viewer and
  history exporter do not branch on game or widget names.

Other games/HUDs are added through locator assets, graph stages and host-registered detectors.
The strategy adapter consumes observations separately. See [the perception guide](perception/README.md).

## Frozen five-frame inspector

**Adjust viewport** pauses application frame acquisition, video playback and new analysis
dispatch, retaining the existing share permission. An already-running WASM job may finish,
but its result cannot produce stale speech. The editor uses frozen images only.

- Up to five most recent full-source frames are retained in memory; no duplicate frames are
  invented to fill empty history slots. The newest analyzed frame is selected when available.
- Choose a thumbnail to inspect that image's own detection report. Cyan boxes were located;
  dashed amber boxes are configured search regions. Their labels/provenance come from reports.
- The source image and cropped preview show the same element overlays. Missing reports say so.
- Automatic-profile inspection does not change the live capture viewport. Its selection is
  an inspection region; closing resumes whole-source discovery. Legacy viewport editing remains
  an explicit compatibility/authoring operation.
- History rasters are bounded to 1920×1440 while preserving aspect; original dimensions remain
  in metadata. Eviction/end-of-share releases the image bitmaps.
- **Export inspection sample** writes one local JSON document with PNG `image_base64`, timestamp,
  profile/revision, source dimensions, capture viewport, inspection selection and normalized boxes.
  It is marked `annotation_state: unreviewed`; automatically produced boxes are not ground truth.
  Its image/timestamp fields are consumable by the native JSONL perception interface.

The browser/source may continue to maintain its screen-share stream internally. The application
does not ingest or add new frames while the inspector is open, and no new share prompt is needed
on resume.

## Validation and next data

Native tests cover a 1600×900 fixture embedded/resized into a 2500×1500 window, translated
video content, independently moved currency text, explicit field-box binding and blank-window
abstention. Browser tests cover an offset video inside a 1920×1200 window with no viewport dialog,
and a five-frame history that stops advancing during inspection. Existing legacy tests remain.

This is not broad cross-game training/evaluation. Next useful data include different UI scales,
themes, minimap layouts, portrait variants, stream overlays and negative screens. Review/correct
exported samples before using them as labels; keep a separate held-out set. The existing examples
bootstrap the prototype, while broader reliability requires additional representative annotations.
