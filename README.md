# StrongSnake

A Rust workspace for building, testing, and tuning a Battlesnake-style agent.

It includes:
- a native GUI playground
- an HTTP move server
- a regression harness for saved scenarios
- an arena runner for bulk matches
- a weight trainer: fits a quadratic model of score against the weights to side-swapped paired games and reports whether its result is statistically proven

## Quick Start

```bash
# GUI (default if no subcommand is provided)
cargo run --release

# HTTP server
cargo run --release -- server --host 0.0.0.0 --port 9000

# Regression suite against scenarios in data/scenarios
# (the GUI is an optional feature, off by default: add --features gui to open the window)
cargo run --release -- test --depths 6

# Arena batch run
cargo run --release -- arena --games 50 --opponent local

# Trainer
cargo run --release -- trainer --depth 5 --gens 40 --pairs 48   # --boards, --candidates, --sigma, --sigma-end, --validation-pairs, --save, --checkpoint/--resume
```

## How The AI Picks Moves

At a high level:
- multi-threaded simultaneous-move search (max-min over both snakes' moves) with alpha-beta pruning; each turn's move pair is resolved with the engine's rules
- iterative deepening: searches depth 1, 2, ... up to `max_depth`, each pass seeding the next one's move ordering (stops early on a forced win)
- transposition table + Zobrist hashing for reuse
- principal variation search
- evaluation based on territory control (Voronoi), flood fill safety, food urgency, and snake length

The default config lives in `crates/snake-ai/src/config.rs` and can be tuned at runtime (CLI server config updates, or the trainer).

## Scenarios

Regression scenarios live in `data/scenarios/*.json`.

Each scenario defines a board state plus an expectation:
- exact move: `{"kind":"exact","direction":"left"}`
- forbidden moves: `{"kind":"avoid","directions":["up","left"]}`

## Workspace Layout

- `src` - GUI + CLI entrypoint (default GUI, `server`, `test`, `arena`, `trainer`)
- `crates/snake-domain` - game types, deterministic simulation engine, food spawn rules, RNG
- `crates/snake-ai` - search and scoring engine
- `crates/snake-api` - standard/legacy request parsing + payload builders
- `crates/snake-io` - scenario schema and load/save helpers
