#!/usr/bin/env bash
# Builds ./cand and ./base: two copies of the engine crates, each with one named variant applied. The harness plays
# one against the other. CAND and BASE name the variants (default: cand = verify_proofs, base = none).
#
#   none         the engine as it is
#   no_hopeful   the hopeful-move step (prefer_hopeful_move) never changes the move
#   hashfix      the Zobrist hash also tells a snake's head from its tail, so reversed bodies on the same cells stop
#                colliding (bench-depth/hashfix.patch)
#   sym_len, sym_tail, sym_trap, sym_food, sym_all, no_food
#                eval-symmetry variants (bench-depth/eval_variants.py explains each): the eval scores some terms from my
#                side only, and each variant makes one of them symmetric. sym_all is the four sym_* together.
#   verify_proofs  a proven win or loss (a decided root score) is searched again with the exact hash of `hashfix`, and
#                that search decides the move; every other move keeps the old hash (bench-depth/verify_proofs.patch)
#   rev_<sha>    the engine (snake-ai, snake-domain) of that commit, e.g. rev_2bf6527; first in a combination. Needs full git
#                history. Use it for `cand` only: the harness plays the games with `base`'s rules (start rules etc.), which
#                an older commit does not have. The harness is built without the asym diagnostic when one is used.
#   patch        bench-depth/candidate.patch applied (a diff against the repo root; the file is not shipped)
#
# Variants combine with `+`: `hashfix+no_hopeful` is the exact hash with the hopeful-move step off.
set -euo pipefail
cd "$(dirname "$0")"
CAND="${CAND:-verify_proofs}"
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
  local d=$1 search=$1/crates/snake-ai/src/search.rs
  case "$2" in
    none) ;;
    no_hopeful) swap "$search" "if depth == 0 || !is_proven_loss(root.score, cfg) {" "if true {" ;;
    hashfix) patch -p1 -d "$d" --no-backup-if-mismatch < hashfix.patch ;;
    verify_proofs) patch -p1 -d "$d" --no-backup-if-mismatch < verify_proofs.patch ;;
    patch) patch -p1 -d "$d" --no-backup-if-mismatch < candidate.patch ;;
    rev_*) # the engine of an older commit (needs full git history); put it first in a combination
      rm -rf "${d:?}/crates/snake-domain" "${d:?}/crates/snake-ai"
      git -C .. archive "${2#rev_}" crates/snake-domain crates/snake-ai | tar -x -m -C "$d" ;; # -m: fresh mtimes, git archive stamps the commit time and cargo would reuse a stale build
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
# The symmetry diagnostic calls the eval directly, whose signature differs in older engines.
mkdir -p .cargo
if [[ "$CAND+$BASE" == *rev_* ]]; then
  printf '[build]\nrustflags = ["--cfg", "no_asym"]\n' > .cargo/config.toml
else
  rm -f .cargo/config.toml
fi
echo "ready: cand = $CAND, base = $BASE"
