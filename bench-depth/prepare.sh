#!/usr/bin/env bash
# Builds ./cand and ./base: two copies of the engine crates, each with one named variant applied. The harness plays
# one against the other. CAND and BASE name the variants (default: cand = none, base = none).
#
#   none         the engine as it is (it already has sym_tail, vor_len, vor_time, vor_eat and the head/tail Zobrist keys)
#   no_hopeful   the hopeful-move step (prefer_hopeful_move) never changes the move
#   sym_len, sym_trap, no_food, no_seal, no_seal_me, no_seal_enemy, no_starve
#                eval variants (bench-depth/eval_variants.py explains each): two make a one-sided term symmetric, the
#                no_* ones drop a term. (sym_tail, vor_len, vor_time and vor_eat are in the engine now.)
#   vor_time_<N> the Voronoi expansion frees body segments for the first N turns only (the engine frees all of them)
#   no_qext      no quiescence extension (the dense-endgame leaf extension in search.rs)
#   qext_<N>     at most N quiescence extensions per line instead of the engine's value (qext_0 is no_qext)
#   dense_occ_<N>  the dense-race trigger (extension and verdict shrink) at N percent board occupancy instead of 40
#   dense_min_<N>  the dense-race trigger needs both snakes at least N long instead of 20
#   no_dense_shrink  trap verdicts keep their full size in a dense tail race (they are divided by 1000 today)
#   scale_<weight>_<pct>  one default weight of AiConfig at <pct>% of its value (bench-depth/weight_variants.py): weights are
#                length, eat, terr, fint, fthr, fexp (food curve), trap, etrap; e.g. scale_terr_140. Same depth, same cost.
#   rev_<sha>    the engine (snake-ai, snake-domain) of that commit, e.g. rev_2bf6527; first in a combination. Needs full git
#                history. Use it for `cand` only: the harness plays the games with `base`'s rules (start rules etc.), which
#                an older commit does not have. The harness is built without the asym diagnostic when one is used.
#   small_tt, skip_iter, lmr_mild   search variants (bench-depth/search_variants.py explains each): a smaller transposition
#                table (speed only), no iteration just before the last one, mild late-move reductions
#   patch        bench-depth/candidate.patch applied (a diff against the repo root; the file is not shipped)
#
# Variants combine with `+`: `qext_5+no_hopeful` is the extension cap 5 with the hopeful-move step off.
set -euo pipefail
cd "$(dirname "$0")"
CAND="${CAND:-none}"
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

# Sets `const NAME: usize = <old>;` to a new value, whatever the old one is; fails loudly if the constant is not there.
swap_const() { # file name value
  python3 - "$1" "$2" "$3" <<'PY'
import re, sys
path, name, value = sys.argv[1:]
text = open(path).read()
new, n = re.subn(rf"(const {name}: usize = )[^;]+;", rf"\g<1>{value};", text, count=1)
if n != 1:
    sys.exit(f"variant needs `const {name}: usize = ...;` in {path}, which is not there")
open(path, "w").write(new)
PY
}

apply_variant() { # dir variant
  local d=$1 search=$1/crates/snake-ai/src/search.rs
  case "$2" in
    none) ;;
    no_hopeful) swap "$search" "if depth == 0 || !is_proven_loss(root.score, cfg) {" "if true {" ;;
    patch) patch -p1 -d "$d" --no-backup-if-mismatch < candidate.patch ;;
    rev_*) # the engine of an older commit (needs full git history); put it first in a combination
      rm -rf "${d:?}/crates/snake-domain" "${d:?}/crates/snake-ai"
      git -C .. archive "${2#rev_}" crates/snake-domain crates/snake-ai | tar -x -m -C "$d" ;; # -m: fresh mtimes, git archive stamps the commit time and cargo would reuse a stale build
    no_qext) swap "$search" "if ctx.root_depth >= 2 && q_depth < QUIESCENCE_MAX_EXTENSIONS &&" "if false && ctx.root_depth >= 2 && q_depth < QUIESCENCE_MAX_EXTENSIONS &&" ;;
    qext_*) swap_const "$search" QUIESCENCE_MAX_EXTENSIONS "${2#qext_}" ;;
    vor_time_*) swap_const "$d/crates/snake-ai/src/voronoi.rs" RELEASE_TURNS "${2#vor_time_}" ;;
    dense_occ_*) swap "$d/crates/snake-ai/src/config.rs" "dense_tail_race_occupancy: 40," "dense_tail_race_occupancy: ${2#dense_occ_}," ;;
    dense_min_*) swap_const "$d/crates/snake-ai/src/heuristics.rs" DENSE_MIN_LEN "${2#dense_min_}" ;;
    no_dense_shrink) swap "$d/crates/snake-ai/src/heuristics.rs" "const DENSE_VERDICT_DIVISOR: i32 = 1000;" "const DENSE_VERDICT_DIVISOR: i32 = 1;" ;;
    scale_*) # scale_<weight>_<percent>, e.g. scale_terr_70: that default weight at 70% of its value
      IFS=_ read -r _ w pct <<< "$2"
      python3 weight_variants.py "$d/crates/snake-ai/src/config.rs" "$w" "$pct" ;;
    sym_*|no_food|no_seal*|no_starve) python3 eval_variants.py "$d/crates/snake-ai/src/heuristics.rs" "$2" ;;
    small_tt|skip_iter|lmr_mild) python3 search_variants.py "$d/crates/snake-ai/src" "$2" ;;
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
# The symmetry diagnostic calls the eval directly, whose signature differs in older engines.
mkdir -p .cargo
if [[ "$CAND+$BASE" == *rev_* ]]; then
  printf '[build]\nrustflags = ["--cfg", "no_asym"]\n' > .cargo/config.toml
else
  rm -f .cargo/config.toml
fi
echo "ready: cand = $CAND, base = $BASE"
