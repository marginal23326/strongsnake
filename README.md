# StrongSnake

A Rust workspace for building, testing, and tuning a Battlesnake-style agent.

It includes:
- a native GUI playground
- an HTTP move server
- a regression harness for saved scenarios
- an arena runner for bulk matches
- a small genetic tuner for evaluation weights

## Quick Start

```bash
# GUI (default if no subcommand is provided)
cargo run --release

# HTTP server
cargo run --release -- server --host 0.0.0.0 --port 9000

# Regression suite against scenarios in data/scenarios
cargo run --release -- test --depths 6

# Arena batch run
cargo run --release -- arena --games 50 --opponent local

# Trainer
cargo run --release -- trainer --pop 30 --gens 25
```

## How The AI Picks Moves

At a high level:
- multi-threaded negamax search with alpha-beta pruning
- transposition table + Zobrist hashing for reuse
- principal variation search with some late-move reduction
- evaluation based on territory control (Voronoi), flood fill safety, food urgency, snake length, and head-to-head pressure

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
