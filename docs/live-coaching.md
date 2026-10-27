# Faster live coaching

## Two independent analysis paths

**Screenshot interval** defaults to 1 and accepts 1–30 seconds. Existing saved preferences
remain saved; choose 1 in Settings to replace an older five-second preference. Acquisition
no longer waits for full OCR to finish.

- The full worker handles discovery, detailed scene interpretation, inventory, hero assignment
  and earned-souls labels. One full job is allowed at a time; busy frames are skipped, not queued.
- A separate recognition-only WASM worker uses registered gameplay/HP/map geometry from a
  recent full result. In one recognition batch it reads current HP, clock (liveness only), and
  menu/death guards, and it analyzes the current minimap. Top-bar health status comes from the
  full scan's round-robin; unobserved slots stay unknown, never safe. It loads only the recognition model and portrait templates.
- The fast worker also has one job in flight and no pixel backlog. Rich map/team/economy
  context expires after 30 seconds. During recovery, conservative current-health/clock reads
  may continue for up to 90 seconds with matching clock and menu/death guards; stale strategic
  fields are removed. Changed source dimensions or invalid readings suppress output. A
  five-second timeout releases the worker.
- Three minimap observations can accumulate over three sampled frames rather than three
  expensive full OCR cycles, improving wave/player-lane stabilization time.
- Inspection/pause/stop halts dispatch; generation checks reject old-session results. History
  retains five actual recent screenshots. Fast-read boxes use **configured/tracked** provenance.

The fast worker also feeds the minimap hero tracker every sample, which is what makes gank
and "missing" calls possible (see [minimap detection](minimap-implementation.md)). With hero-ring
search added, a fast update measured ~0.93 s in the headless browser suite.
Native fixture monitoring measured about 154 ms before ring search. A real-WASM browser test measured 567–750 ms
per fast update, acquiring 19–20 screenshots during two full scans. This is headless fixture
evidence, not a foreground-game FPS or thermal benchmark. Background scheduling/load can exceed
the one-second target. Diagnostics display both durations; `coach:capture` / `coach:quick`
events expose cadence and results. The fast path needs an initial registered full result.

## Fast perception framework

Full-scene analysis used to run the OCR text *detector* (~150 ms natively, 1–1.6 s in WASM per
pass) on many regions every frame. The steady state now detects once, then tracks:

- **Detect once, track exact boxes.** `coach_perception::track::Tracked` keeps a detected box,
  counts hits/misses, and allows re-acquisition at most every 2 s (`may_acquire`). The text
  recognizer needs tight, exactly detected boxes: padding and fixed boxes measured worse.
- **One recognition batch per frame.** `LineBatch` collects every single-line crop that can
  change a decision this frame (screen probe, 4 overlapping clock boxes, HP, souls, urn
  instruction, a rotating subset of team banners). `ocr::read_lines` packs them onto one
  canvas and upscales small crops to ~40 px height. Cost scales with crop width/height.
- **Extrapolated values.** `Extrapolated` anchors the match clock with a verified read, advances
  it with wall time and only *confirms* with cheap reads. Whole-minute misreads (5:13 → 6:13)
  never move it; other disagreements re-verify, throttled to 2 s, plus a 120 s re-verify.
  Verification uses one detector pass on the fixed clock box when it agrees with independent
  evidence (the extrapolation or the window-scale glyph read). Otherwise, or after the fixed box
  has misread once in that context, it locates the glyphs and votes on glyph-scaled reads.
- **Round-robin and cadences.** `RoundRobin` spreads slow fields (team banners 3/12, earned
  souls 2/12 per frame). `Cadence` throttles rare detector work: scoreboard names on a pixel
  panel cue or every 15 s, match ID every 60 s, death cause once per death.
- **Parallel recognition in the browser.** With cross-origin isolation, a `LinePool` of up to
  four recognition-only workers splits each batch by estimated cost via `SharedArrayBuffer`.
  The first visit reloads once through the service worker to enable COOP/COEP; non-isolated
  sessions fall back to single-threaded recognition (~2× slower). A worker stalled for 5 s
  marks the pool broken and falls back.
- **Per-phase timing.** `Laps` records each phase. Diagnostics show "Slowest scene phases"
  and pool status; `examples/profile_scan.rs` prints the same natively.

Measured (headless fixtures, this machine): the browser replay's last frame takes 1.4–1.8 s
including a rare ~1.1–1.4 s clock verification, a 12-line batch takes 170–280 ms with
4 parallel recognizers, the auto window batch 66–83 ms, and the fast monitor 0.5–0.8 s.
Before this work, a frame took 2.8–4.9 s, with past runs up to ~18 s. Natively single-threaded
(`RTEN_NUM_THREADS=1`), steady state is ~0.4 s calibrated and ~0.6–0.9 s automatic. Limits:
the first frame still needs detector acquisition (~4–6 s single-threaded WASM); heavily
softened (~720p upscaled) streams can misread the clock minute even with the detector, so only
verification moves the clock. Live in-game FPS and thermal impact remain unmeasured.

## Full-worker timeout recovery

Game feed now labels **Full scene** and **Fast HP/map** separately: an 11,869 ms full-scene
reading really is about 11.9 seconds of processing, not the fast monitor's latency.

Live full scans have a 20-second watchdog once a usable fast context exists; cold acquisition
and replay retain 45 seconds. On a live Deadlock failure, only the full worker is terminated.
The same capture stream and screenshot loop remain active. The full worker is rebuilt with
two bounded automatic retries; exhausted retries are visible and can be restarted in Settings.
The fast worker continues only with valid, age-gated evidence. No stale gank/farming state is
retained to make recovery look successful.

Automatic browser input is capped at 1920 pixels wide while keeping the whole source and its
aspect ratio. This reduces 4K work without asking for screen-size settings or a manual crop.
Minimap fallback search uses per-scale coarse proposal budgets and six refined peaks; refinements
scale their stride with source size. Flat images abstain early, map proposals reuse one grayscale
frame, and failed text localization has a small attempt budget. Scoreboard name OCR is batched
into vertically stacked label crops rather than repeated per hero. Menu/death text uses verified
tight line crops. Saved-frame classification, names and detector regressions still pass.

## Earlier and useful speech

- Power-up/Urn preparation starts 45 seconds before scheduled spawn; boost unlock starts 30
  seconds before. Processing age is subtracted before speech. Countdowns under 15 seconds are
  dropped, including after waiting behind another utterance.
- Back-off warnings start below the heuristic 45% health threshold or after an observed drop
  above eight percentage points. They interrupt the normal cue gap immediately. Health speech
  over 2.5 seconds old is discarded; high-priority speech preempts an existing utterance.
- Wave reports summarize pressure across lanes even without confirmed own identity. Positive
  wave-position/farm-pace cues recognize useful states, not unobserved kills or camp clears.
- Wall-time speech deduplication spans both workers and survives option changes in a session.

## Health, ganks and earned souls

All twelve top-bar slots have explicit health observations. The reader recognizes the literal
**CRITICAL** label and distinguishes a non-red/non-critical banner from an unreadable banner.
It does **not** derive exact percentages for every hero: `health_ratio` stays null. "Not critical"
is not "healthy/full HP". Own numeric current/max HP remains a separate reader.

Portrait-to-minimap matching is conservative and experimental: low similarity/margin or duplicate
candidates leave positions unknown. A conditional gank needs own healthy readings and a crashed
wave, a repeatedly critical enemy with a repeatedly matched position in another nearby lane,
nearby located non-critical allied support, and no endangered wave/imminent objective conflict.
Unknown identity/position/health never becomes a specific gank order. Exact HP, cooldowns, hidden
enemies and path safety remain missing evidence. Camp suggestions stay conditional on availability.
See the document-backed [lane policy](active-laning.md).

Top-bar **earned net worth** is separate from unspent Souls. Label crops are stacked for one OCR
pass, associated back to slots, and temporally checked. Auto identity uses a confident own-map
portrait; the optional **Your hero** selector confirms identity when auto matching is unavailable.
A personal comparison needs that identity, two plausible readings and at least six comparable
hero totals. Spending unspent currency must not look like losing earned income.

Behind/on-track/ahead bands compare with the observed lobby median after three minutes (0.8 /
1.2 ratios are coach heuristics), with two-minute cooldowns. They are not universal rank/hero
target curves. Displayed total-souls-per-minute includes starting balance, not instantaneous
farming rate. Missing totals remain unknown instead of substituting unspent balance.
