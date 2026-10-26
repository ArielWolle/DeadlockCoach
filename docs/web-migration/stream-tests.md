# Twitch stream smoke tests

These are real public-stream frames processed through the running browser/WASM app.
They are **screenshot replay tests**, not validation of continuous `getDisplayMedia` capture,
background scheduling, audio routing, or advice quality throughout a complete match.
No full stream recordings or new creator screenshots are included in the published assets.

## Monkeymires — 1080p

Source selected by the owner: <https://www.twitch.tv/monkeymires>.
The HTML video reported **1920×1080** playback. The player was displayed inside a
1920×1143 viewport with `object-fit: contain`, giving a 31.5-pixel upper letterbox.
Test screenshots clipped the actual video rectangle `(0, 31.5, 1920, 1080)`, not the
whole browser/player element. A temporary transparent DOM target defined the clipping box;
it disappears on page reload. The browser relay disconnected after result inspection, so
explicit removal of that target could not be confirmed.

Two distinct JPEG frames (quality 85) were imported together into the app's Replay lab.
The app used its normal main-HUD scaling, native-resolution minimap crop, actual OCR,
state tracking and coaching engine. It was not supplied prewritten snapshots or OCR values.

| Field | Visible second frame | Browser result |
| --- | --- | --- |
| Game clock | 34:42 | 34:42 |
| Unspent souls | 2,669 | 2,669 |
| Screen | Normal gameplay | Gameplay |
| Team net worth | About 266k / 229k | Not populated; requires investigation |
| Inventory | Both rows contain items | Six item IDs returned; full item accuracy not validated |
| Hero roster | Portraits visible | Not populated during two-frame warmup; needs a longer sample |
| Minimap | Visible at the calibrated lower-right position | Two observations; temporal map-rule threshold of three not reached |
| Match ID | Visible at lower-right | OCR omitted leading digits; exact ID not validated |

The rule engine generated **“Soul Urn in 18 seconds”** at 34:42, matching its configured
35:00 scheduled reminder. Replay is silent, so this does not verify audible output.
Processing took **7,731 ms** and **15,145 ms** for the two frames in this browser session.
These are observed timings, not a guaranteed processing budget or live-game FPS measurement.

Locally retained source-frame SHA-256 values for this test:
- `68d4972e80fb99a673e11b6b27884b7351cfb45ad52855740a0d5b021cdaa737`
- `987ee5c89601bd3b86264825efe7f74f7ed7195e8ebdb28a2bade3bb160cabf0`

### Continuous stream setup

Select the Twitch tab/window through the browser's screen-sharing picker. The selected
capture needs the game video to fill a 16:9 viewport; a fullscreen player can still have
letterboxing on a 16:10 screen. Use capture calibration to exclude those bars. For the
specific 1920×1143 geometry above, the approximate viewport percentages are left 0%, top
2.8%, width 100%, height 94.4%. These values depend on the actual shared source dimensions;
they are not a universal Twitch preset and were not saved over the owner's preferences.

## Praydonnis — 720p diagnostic, subsequently paused

Source selected by the owner: <https://www.twitch.tv/praydonnis>.
Playback reported **1280×720**; the page rendered it at about 1340×754. Two player-only
screenshots were imported into Replay lab. Visible clocks were 3:32 / 3:34 and souls were
1,330, but OCR could not reliably read the small compressed clock. Both snapshots were
classified Unknown, emitted no gameplay data and produced no coaching calls.

A native OCR probe confirmed that souls were legible but clock recognition was unreliable.
Tighter crops/contrast variants did not establish a reliable fallback. No weakened parser
or guessed clock values were introduced. The owner chose to establish a 1080p baseline
before spending further work on 720p support.

## Implications for a broader test matrix

- Streams provide varied heroes, builds, HUD settings, overlays, capture scales and situations.
- Match playback resolution, rendered video rectangle and browser capture dimensions are
  separate measurements; recording only a stream's advertised quality is insufficient.
- Browser speed depends on the viewer's computer. Streamer hardware/game FPS is not a
  benchmark of the computer running the coach.
- Next useful checks: a longer 1080p sample for roster/minimap confirmation, scoreboard/death
  transitions, team totals and exact item comparisons, followed by continuous capture.
