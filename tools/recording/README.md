# Recording analysis

Statistical checks of what the coach read during a real match, and a deterministic replay of
the same screenshots through the current pipeline to measure fixes.

1. In the app, enable **Record**, play, then export **frames + snapshots** (`deadlock-recording.json`;
   up to 600 frames, about ten minutes). The export also contains `spoken`: every cue said by
   either analysis path, with game clock and your HP at that moment.
2. `python tools/recording/extract.py deadlock-recording.json frames/rec1/img`
3. `cargo run --release -p coach-vision --example replay_dir -- frames/rec1/img frames/rec1/replay.jsonl`
4. `python tools/recording/stats.py frames/rec1/replay.jsonl` (or the export itself) for per-field
   statistics: own HP reads, top-bar state sequences and flips per slot, health jitter, lane fronts,
   trooper counts, player marker rate, and the cues the engine would speak.
5. `python tools/recording/compare.py deadlock-recording.json frames/rec1/replay.jsonl` for a
   live-vs-replay summary.

Keep recordings in the git-ignored `frames/` directory. Replay decodes the exported JPEGs, so it
is slightly harder than live analysis of raw canvas pixels.
