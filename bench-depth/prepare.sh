#!/usr/bin/env bash
# Builds ./cand and ./base: two copies of the engine crates, each with one named variant applied. The harness plays
# one against the other. CAND and BASE name the variants (default: cand = lmr_off, base = none).
#
#   none         the engine as it is
#   lmr_off      late-move reductions off
#   lmr_d4       late-move reductions only from remaining depth 4 (the engine uses 3)
#   lmr_d5       ... from remaining depth 5
#   lmr_r2       late-move reductions that cut 2 turns instead of 1 (a probe then ends on the same side of an even/odd
#                horizon as the full search it replaces)
#   lmr_no_root  late-move reductions everywhere except the root node (the root's child scores are what the
#                hopeful-move logic reads, and reduced probes leave only a bound there)
#   no_hopeful   the hopeful-move step (prefer_hopeful_move) never changes the move
#   force_w3     the bitboards always use 3 words (192 cells) instead of 2 for boards up to 128 cells
#   force_w7     the bitboards always use 7 words (448 cells), whatever the board
#   id           iterative deepening (bench-depth/id.patch): depth 1, 2, ... up to the fixed depth, each pass seeding the next
#   qrule        (needs id) iterative deepening refuses the score of an entry from an earlier iteration that had spent more
#                quiescence extensions than the search reading it; its moves still seed the move ordering
#   qany         the same refusal, but whatever iteration stored the entry: a score that was reached with more quiescence
#                extensions spent than the search reading it has is never reused. Works without id (qrule alone is
#                inert there, because a fixed-depth search has only one iteration)
#   id_no_winexit      (needs id) never stop early on a forced win: always search every depth up to the fixed one
#   id_fresh_history   (needs id) clear the history table before every iteration, so no pass inherits cutoff history
#   id_no_root_hint    (needs id) the root's own move ordering ignores the previous pass's best move
#   exact_depth  the transposition table only answers with an entry searched to exactly the remaining depth, never a
#                deeper one (so a pass cannot borrow extra depth from an earlier, deeper search of the same position)
#   hashfix      the Zobrist hash also tells a snake's head from its tail, so reversed bodies on the same cells stop
#                colliding (bench-depth/hashfix.patch)
#   sym_len, sym_tail, sym_trap, sym_food, sym_all, no_food
#                eval-symmetry variants (bench-depth/eval_variants.py explains each): the eval scores some terms from my
#                side only, and each variant makes one of them symmetric. sym_all is the four sym_* together.
#   patch        bench-depth/candidate.patch applied (a diff against the repo root; the file is not shipped)
#
# Variants combine with `+`: `lmr_off+id` is iterative deepening with late-move reductions off.
#
# force_w3 / force_w7 must not change a single move: only the integer type the board is stored in differs. Played
# against `none` on the same board, every pair has to score exactly 50%, and any other result is a bug in the
# multi-word bitboard code.
set -euo pipefail
cd "$(dirname "$0")"
CAND="${CAND:-lmr_off}"
BASE="${BASE:-none}"

# Rewrites `file` and fails loudly if the text it was meant to change is not there (the engine source moved on).
swap() { # file old new
  python3 - "$1" "$2" "$3" <<'PY'
import sys
path, old, new = sys.argv[1:]
text = open(path).read()
if old not in text:
    sys.exit(f"variant needs `{old}` in {path}, which is not there")
open(path, "w").write(text.replace(old, new, 1))
PY
}

apply_variant() { # dir variant
  local d=$1 search=$1/crates/snake-ai/src/search.rs brain=$1/crates/snake-ai/src/brain.rs
  case "$2" in
    none) ;;
    lmr_off) swap "$search" "const LMR_MIN_DEPTH: usize = 3;" "const LMR_MIN_DEPTH: usize = usize::MAX;" ;;
    lmr_d4) swap "$search" "const LMR_MIN_DEPTH: usize = 3;" "const LMR_MIN_DEPTH: usize = 4;" ;;
    lmr_d5) swap "$search" "const LMR_MIN_DEPTH: usize = 3;" "const LMR_MIN_DEPTH: usize = 5;" ;;
    lmr_r2) swap "$search" "const LMR_REDUCTION: usize = 1;" "const LMR_REDUCTION: usize = 2;" ;;
    lmr_no_root) swap "$search" "if depth >= LMR_MIN_DEPTH && !ate_food {" "if depth >= LMR_MIN_DEPTH && !ate_food && !is_root {" ;;
    no_hopeful) swap "$search" "if depth == 0 || !is_proven_loss(root.score, cfg) {" "if true {" ;;
    force_w3)
      swap "$brain" "0..=2 => decide_move_inner::<2, 128>(" "0..=2 => decide_move_inner::<3, 192>(" ;;
    force_w7)
      swap "$brain" "0..=2 => decide_move_inner::<2, 128>(" "0..=2 => decide_move_inner::<7, 448>("
      swap "$brain" "3 => decide_move_inner::<3, 192>(" "3 => decide_move_inner::<7, 448>(" ;;
    id) patch -p1 -d "$d" --no-backup-if-mismatch < id.patch ;;
    qrule) patch -p1 -d "$d" --no-backup-if-mismatch < qrule.patch ;;
    qany)
      patch -p1 -d "$d" --no-backup-if-mismatch < qrule.patch
      swap "$search" "&& !(entry.iteration < self.root_depth.min(255) as u8 && entry.q_used as usize > q_depth)" "&& entry.q_used as usize <= q_depth" ;;
    id_no_winexit) swap "$search" "if depth >= max_depth || forced_win {" "if depth >= max_depth || (forced_win && false) {" ;;
    id_fresh_history) swap "$search" "        ctx.root_depth = depth;
        let result = search(" "        ctx.root_depth = depth;
        ctx.history_table.iter_mut().for_each(|t| t.fill(0));
        let result = search(" ;;
    id_no_root_hint) swap "$search" "tt_entry.and_then(|e| e.get_direction())," "tt_entry.filter(|_| !is_root).and_then(|e| e.get_direction())," ;;
    exact_depth) swap "$search" "(entry.depth as usize) >= depth" "(entry.depth as usize) == depth" ;;
    hashfix) patch -p1 -d "$d" --no-backup-if-mismatch < hashfix.patch ;;
    patch) patch -p1 -d "$d" --no-backup-if-mismatch < candidate.patch ;;
    sym_*|no_food) python3 eval_variants.py "$d/crates/snake-ai/src/heuristics.rs" "$2" ;;
    *) echo "unknown variant: $2" >&2; exit 1 ;;
  esac
}

rm -rf cand base
for pair in "cand:$CAND" "base:$BASE"; do
  d=${pair%%:*}
  mkdir -p $d/crates
  cp -r ../crates/snake-domain ../crates/snake-ai $d/crates/
  cat > $d/Cargo.toml <<'TOML'
[workspace]
members = ["crates/snake-domain", "crates/snake-ai"]
resolver = "2"

[workspace.package]
edition = "2024"
license = "MIT"
version = "0.1.0"
TOML
  IFS=+ read -ra variants <<< "${pair#*:}"
  for v in "${variants[@]}"; do apply_variant "$d" "$v"; done
done
# Cargo cannot hold two different packages with the same name in one lockfile, so the candidate gets its own names.
sed -i 's/^name = "snake-domain"/name = "snake-domain-cand"/' cand/crates/snake-domain/Cargo.toml
sed -i 's/^name = "snake-ai"/name = "snake-ai-cand"/' cand/crates/snake-ai/Cargo.toml
sed -i 's|^snake-domain = .*|snake-domain = { package = "snake-domain-cand", path = "../snake-domain" }|' cand/crates/snake-ai/Cargo.toml
echo "ready: cand = $CAND, base = $BASE"
