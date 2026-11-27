#!/usr/bin/env bash
# Builds ./cand and ./base: two copies of the engine crates, each with one named variant applied. The harness plays
# one against the other. CAND and BASE name the variants (default: cand = none, base = none).
#
#   none         the engine as it is (it already has sym_tail, vor_len, vor_time, vor_eat and the head/tail Zobrist keys)
#   no_hopeful   the hopeful-move step (prefer_hopeful_move) never changes the move
#   food_fade_<LO>_<HI>
#                eval variant (bench-depth/eval_variants.py explains it): fades the food pull as the board fills. (sym_len,
#                sym_tail, vor_len, vor_time and vor_eat are settled or in the engine now, and the seal verdicts are gone.)
#   en_pull_<P>  the enemy gets the engine's food term mirrored (a hungry enemy far from food is good for me), at P percent; changes play
#   cramp_sq_<P> the room term's shortfall s counts as s + s*s*P/100 (convex); changes play
#   no_cramp, cramp_scale_<P>, cramp_c_<C>  the room term (a snake with under 1.3 Voronoi cells per body segment is penalised;
#                it is in the engine): no_cramp removes it, cramp_scale_<P> sets its weight to P percent, cramp_c_<C> its threshold to C/10
#   vor_time_<N> the Voronoi expansion frees body segments for the first N turns only (the engine uses 16)
#   no_qext      no quiescence extension (the dense-endgame leaf extension in search.rs)
#   qext_<N>     at most N quiescence extensions per line instead of the engine's value (qext_0 is no_qext)
#   dense_min_<N>  the quiescence extension runs when both snakes are at least N long (the engine: 16)
#   scale_<weight>_<pct>  one default weight of AiConfig at <pct>% of its value (bench-depth/weight_variants.py): weights are
#                length, eat, urg (eat_urgency), terr, fint, fthr, fexp (food curve); e.g. scale_terr_140. Same depth, same cost.
#   set_<weight>_<value>  one default weight set to that value (trainer results). Same depth, same cost.
#   old_weights  the weights from before the trainer's depth-5 result went in (length 1000, eat 2000, food 3303.092/19.357/1.968)
#   prev_weights  the weights before the trainer's second result became the defaults (length 1903, eat 6436, urgency 50,
#                food 2553.9771/36.657/2.1932). Same cost.
#   rev_<sha>    the engine (snake-ai, snake-domain) of that commit, e.g. rev_2bf6527; first in a combination. Needs full git
#                history. Use it for `cand` only: the harness plays the games with `base`'s rules (start rules etc.), which
#                an older commit does not have. The harness is built without the asym diagnostic when one is used.
#   lmr_mild     mild late-move reductions (bench-depth/search_variants.py explains it)
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
    dense_min_*) swap_const "$d/crates/snake-ai/src/heuristics.rs" DENSE_MIN_LEN "${2#dense_min_}" ;;
    scale_*) # scale_<weight>_<percent>, e.g. scale_terr_70: that default weight at 70% of its value
      IFS=_ read -r _ w pct <<< "$2"
      python3 weight_variants.py "$d/crates/snake-ai/src/config.rs" "$w" "$pct" ;;
    set_*) # set_<weight>_<value>, e.g. set_eat_6436: that default weight set to the value (a trainer result)
      IFS=_ read -r _ w val <<< "$2"
      python3 weight_variants.py "$d/crates/snake-ai/src/config.rs" "$w" "$val" set ;;
    old_weights) # the defaults before the trainer's depth-5 result (validated +22 Elo at depth 6) became the defaults
      for kv in eat=2000 length=1000 fint=3303.092 fthr=19.357 fexp=1.968; do
        python3 weight_variants.py "$d/crates/snake-ai/src/config.rs" "${kv%%=*}" "${kv#*=}" set
      done ;;
    prev_weights) # the defaults before the trainer's second result (validated +21.7 at depth 6, +5.9 at depth 8) became the defaults
      for kv in eat=6436 length=1903 urg=50 fint=2553.9771 fthr=36.657 fexp=2.1932; do
        python3 weight_variants.py "$d/crates/snake-ai/src/config.rs" "${kv%%=*}" "${kv#*=}" set
      done ;;
    food_fade_*|en_pull_*|no_cramp|cramp_scale_*|cramp_c_*|cramp_sq_*) python3 eval_variants.py "$d/crates/snake-ai/src/heuristics.rs" "$2" ;;
    lmr_mild) python3 search_variants.py "$d/crates/snake-ai/src" "$2" ;;
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
