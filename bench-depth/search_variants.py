#!/usr/bin/env python3
"""Search-speed variants for bench-depth/prepare.sh.

    search_variants.py <crates/snake-ai/src> <variant>

  lmr_mild    late-move reductions: at depth >= 4, not at the root, a move after the first two (one that does not eat and
              does not land next to the enemy head) is probed one turn short with a zero window and dropped if the probe
              fails low, otherwise searched as before. About 0.94x nodes, root move differs in about 2.7% of positions.

Every replacement fails loudly if the text it expects is not in the file (the engine moved on)."""
import os
import sys

LMR_EDITS = ("search.rs", [
    ("const PV_MAX_TURNS: usize = 32;\n", """const PV_MAX_TURNS: usize = 32;

/// Late-move reductions: a move after the first `LMR_FROM_INDEX` is first probed `LMR_REDUCTION` turns short with a
/// zero window. If that probe fails low the move is dropped, otherwise it is searched as before. Never at the root, never
/// for a move that eats or lands next to the enemy head.
const LMR_MIN_DEPTH: usize = 4;
const LMR_FROM_INDEX: usize = 2;
const LMR_REDUCTION: usize = 1;
"""),
    ("    node_depth: usize,\n", "    node_depth: usize,\n    /// Turns the positions below are searched short of `node_depth - 1`.\n    reduction: usize,\n"),
    ("depth: query.node_depth - 1,", "depth: query.node_depth - 1 - query.reduction,"),
    ("""        let probe = |alpha: i32, beta: i32| ReplySearch {
            my_mv: mv,
            replies,
            node_depth: depth,
""", """        let probe = |alpha: i32, beta: i32, reduction: usize| ReplySearch {
            my_mv: mv,
            replies,
            node_depth: depth,
            reduction,
"""),
    ("""            worst_case_reply(grid, me, enemy, &probe(alpha, beta), ctx)
        } else {
            let probed = worst_case_reply(grid, me, enemy, &probe(alpha, alpha + 1), ctx);
            if probed.0 > alpha && probed.0 < beta {
                worst_case_reply(grid, me, enemy, &probe(alpha, beta), ctx)
            } else {
                probed
            }
        };""", """            worst_case_reply(grid, me, enemy, &probe(alpha, beta, 0), ctx)
        } else {
            let reducible = depth >= LMR_MIN_DEPTH && !is_root && i >= LMR_FROM_INDEX && !ate_food && manhattan(mv.cell(), en_head) != 1;
            let reduced = reducible.then(|| worst_case_reply(grid, me, enemy, &probe(alpha, alpha + 1, LMR_REDUCTION), ctx)).filter(|probed| probed.0 <= alpha);
            if let Some(result) = reduced {
                result
            } else {
                let probed = worst_case_reply(grid, me, enemy, &probe(alpha, alpha + 1, 0), ctx);
                if probed.0 > alpha && probed.0 < beta {
                    worst_case_reply(grid, me, enemy, &probe(alpha, beta, 0), ctx)
                } else {
                    probed
                }
            }
        };"""),
])

VARIANTS = {"lmr_mild": LMR_EDITS}


def apply(src_dir, variant):
    if variant not in VARIANTS:
        sys.exit(f"unknown search variant: {variant}")
    name, edits = VARIANTS[variant]
    path = os.path.join(src_dir, name)
    text = open(path).read()
    for old, new in edits:
        if text.count(old) != 1:
            sys.exit(f"variant {variant} needs this text exactly once in {path} (the engine moved on):\n{old}")
        text = text.replace(old, new, 1)
    open(path, "w").write(text)


if __name__ == "__main__":
    if len(sys.argv) != 3:
        sys.exit(__doc__)
    apply(sys.argv[1], sys.argv[2])
