# Faster live coaching

## Two independent analysis paths

**Screenshot interval** defaults to 1 and accepts 1–30 seconds. Existing saved preferences
remain saved; choose 1 in Settings to replace an older five-second preference. Acquisition
no longer waits for full OCR to finish.

- The full worker handles discovery, detailed scene interpretation, inventory, hero assignment
  and earned-souls labels. One full job is allowed at a time; busy frames are skipped, not queued.
- A separate recognition-only WASM worker uses registered gameplay/HP/map geometry from a
  recent full result. It reads current HP, clock, menu/death guards, top-bar health status and
  the current minimap. It loads only the recognition model and portrait templates.
- The fast worker also has one job in flight and no pixel backlog. Rich map/team/economy
  context expires after 30 seconds. During recovery, conservative current-health/clock reads
  may continue for up to 90 seconds with matching clock and menu/death guards; stale strategic
  fields are removed. Changed source dimensions or invalid readings suppress output. A
  five-second timeout releases the worker.
- Three minimap observations can accumulate over three sampled frames rather than three
  expensive full OCR cycles, improving wave/player-lane stabilization time.
- Inspection/pause/stop halts dispatch; generation checks reject old-session results. History
  retains five actual recent screenshots. Fast-read boxes use **configured/tracked** provenance.

Native fixture monitoring measured about 154 ms. A real-WASM browser test measured 567–750 ms
per fast update, acquiring 19–20 screenshots during two full scans. This is headless fixture
evidence, not a foreground-game FPS or thermal benchmark. Background scheduling/load can exceed
the one-second target. Diagnostics display both durations; `coach:capture` / `coach:quick`
events expose cadence and results. The fast path needs an initial registered full result.

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
