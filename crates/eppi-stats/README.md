# eppi-stats

Slippi replay stats for Eppi: a Rust port of slippi-js 9.1.3's `getStats()` and `getWinners()`,
built on peppi 2.1.

```rust
let game = eppi_stats::read_replay(path)?;   // peppi + repair fallback, catches peppi panics
let stats = eppi_stats::compute(&game);       // never panics
let json = serde_json::to_string(&stats)?;    // camelCase, same shape as slippi-js
```

Cache results keyed on `eppi_stats::STATS_VERSION` and recompute when it changes.

## What's computed

`GameStats` holds everything `getStats()` returns (`lastFrame`, `playableFrameCount`, `stocks`,
`conversions`, `combos`, `actionCounts`, `overall`, `gameComplete`) plus:

- `winners`: slippi-js `getWinners()` (LRAS quit-outs, placements, timeouts, teams, draws).
- `players`: each player's stocks and percent on their last frame with data.
- `stocks[].killedBy` / `killMoveId`: the attacker and last move of the conversion that ended
  with the death, found the way the Slippi Launcher's kills table finds it. `killedBy: null`
  means self-destruct; `killedBy` set with `killMoveId: null` is a conversion with no moves
  (e.g. a grab release).

As in slippi-js, stocks, conversions, combos, action counts and input counts are only computed
for singles (exactly two players). `overall` exists for every player in every game.

`melee` has the lookup tables the UI needs (characters and costumes by external id, stages,
move names, death direction, aerial landing lag) and `melee::state` has the action-state ids and
predicates. `melee/tables.rs` is generated from slippi-js's JSON by
`tools/verify-stats/gen-tables.mjs`.

## How it matches slippi-js

- Frames: slippi-js only processes *finalized* frames, keeps the latest copy of a frame resent by
  rollback, and walks from frame -123 until a frame is missing or lacks post-frame data for any
  player. `frames.rs` reproduces this, including the finalization limit for replays without a
  Game End (online: the last bookend's latest finalized frame; otherwise newest frame - 7).
- Numbers: percents are `f32` in the replay and doubles in JavaScript. We do the same
  conversions and the same arithmetic in the same order, so damage sums are bitwise equal.
- Quirks kept on purpose, so numbers match the Launcher: action counts can go negative
  (slippi-js decrements air dodges, dash attacks and failed L-cancels); an opening is never a
  counter-attack when the other conversion ended on frame 0 or hasn't ended; a conversion with
  no moves is compared with its own end frame; `conversionCount` counts every conversion whose
  victim isn't the player; a 1v1 LRAS with no recorded initiator gives the win to the first
  player.

## Intentional differences

- **No Game End means no winners.** slippi-js finds the final post-frame updates by stepping
  back from the end of the raw stream past a Game End event. Without one it reads the wrong
  bytes and returns no winner. We return `[]` directly. This affects cut-off replays only.
- **Characters without frame data** (Master Hand and other special characters, internal ids
  above 0x1A) make slippi-js's edge-cancel check throw, so `getStats()` fails for the whole game.
  We skip only that check and return everything else.
- **Cut-off files ending mid-frame.** slippi-js counts the partial last frame in `lastFrame`. Our
  repair keeps only complete frames, so `lastFrame`, `playableFrameCount` and the per-minute
  rates can be one frame lower. Every frame-based stat is the same, because slippi-js never
  processes that unfinalized frame either.
- **`players` for eliminated players.** In 3-4 player games, slippi-js has no data for
  eliminated players on the last frame. We report their last frame with data.

## Reading replays (`read_replay`)

peppi 2.1.2 rejects some replays that slippi-js reads. `read_replay` parses the whole file from
memory and, only if peppi fails, retries on a repaired copy (`repair.rs`):

- **Cut-off files** (Dolphin crashed, file still being written): the raw stream is cut after the
  last complete frame and the metadata block is dropped. `gameComplete` is `false`.
- **Unknown Game End methods** (newer Slippi writes method 9): parsed with a stand-in method.
  `game.end.method` reads `Resolved`, but `game.end.bytes[0]` keeps the real value, and winners
  use that byte, as slippi-js does.

peppi `assert!`s on some malformed input, so `read_replay` wraps it in `catch_unwind` and
returns `ReadError::Panic`. The default panic hook still prints to stderr.

## Verification

`tools/verify-stats` runs the npm `@slippi/slippi-js` 9.1.3 and this crate on the same replays
and diffs every field:

```sh
cd tools/verify-stats && pnpm install
node sample.mjs <replayDir> 300 out/sample.txt          # or list every file
node run-parallel.mjs out/sample.txt out/slippi 16       # slippi-js JSON
cargo run --release -p eppi-stats --example dump_stats -- tools/verify-stats/out/sample.txt tools/verify-stats/out/eppi
node compare.mjs out/slippi out/eppi
```

### Results (2026-11-08, all 8,644 replays in the owner's Slippi folder, 2025–2026)

- 8,644 of 8,644 replays read. 8 needed the repair fallback: 6 with end method 9 and 2 cut off.
- slippi-js failed on 9 (special characters, see above), leaving 8,635 to compare.
- 8,634 match on every field, plus winners and final stocks/percent. That's 11,323,082 numbers,
  and every number that matched was bitwise equal (no float tolerance needed).
- The one difference is a cut-off file that ends mid-frame (`lastFrame` 695 vs 696, see above).

### Speed

Release build, 32-thread desktop, files in the OS cache, `examples/bench.rs`:

| | ms per replay (wall) | all 8,644 (27.1 GB) |
|---|---|---|
| 1 thread | 8.3 (≈7.3 read + 1.1 compute) | 72 s |
| rayon, 32 threads | 1.5 | 13 s |

With a cold cache, reads will be limited by disk speed. For comparison, slippi-js takes about
300 ms per replay.
