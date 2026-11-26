#!/usr/bin/env python3
"""Eval variants for bench-depth/prepare.sh.

    eval_variants.py <heuristics.rs> <variant>

Each variant below rewrites one piece of the eval in crates/snake-ai/src/heuristics.rs so the arena can say whether it matters:

  food_fade_<LO>_<HI>  the flat pull toward food fades out as the board fills: full below LO percent occupancy (both snakes' lengths
                 over the board's cells), none at HI percent and above, linear between. Only while health is comfortable (buffer over
                 40): below that the full pull stays so snakes still eat in time. Data from self-play (harness dump) says that late in
                 the game, being farther from food than the enemy goes with winning beyond what the score says.

  no_cramp       the room term is removed (undoes the cramp commit; arena: it is worth about +20 Elo).
  cramp_scale_<P>  the room term's weight at P percent of the engine's (cramp_scale_50 half, cramp_scale_200 double).
  cramp_c_<C>    the room term's threshold set to C/10 cells of territory per body segment (cramp_c_12 is 1.2; the engine uses 1.3).
                 Both are for scans on top of the landed term. Symmetric, no measurable cost.

Every replacement fails loudly if the text it expects is not in the file (the eval moved on)."""
import re
import sys

FOOD_USE = "        score -= (closest_dist as f64 * panic_value(&scores.food, buffer)) as i32;\n"

CRAMP_THRESH_OLD = "const CRAMP_THRESHOLD: f64 = 1.3;\n"
CRAMP_WEIGHT_OLD = "* CRAMP_WEIGHT * unit as f64)"
CRAMP_USE = """    let cramp = |count: i32, len: usize| (CRAMP_THRESHOLD - count as f64 / len as f64).max(0.0);
    score += ((cramp(voronoi.enemy_count, enemy.body.len()) - cramp(voronoi.my_count, me.body.len())) * CRAMP_WEIGHT * unit as f64) as i32;
"""

def apply(path, variant):
    text = open(path).read()
    open(path, "w").write(apply_text(text, path, [variant]))


def apply_text(text, path, variants):
    for v in variants:
        if v.startswith("food_fade_") and len(v.split("_")) == 4 and all(p.isdigit() for p in v.split("_")[2:]):
            lo, hi = (int(p) for p in v.split("_")[2:])
            fade = (
                "        let occupancy = (me.body.len() + enemy.body.len()) as f64 * 100.0 / (grid.width * grid.height) as f64;\n"
                f"        let fade = if buffer > 40 {{ (({hi}.0 - occupancy) / ({hi}.0 - {lo}.0)).clamp(0.0, 1.0) }} else {{ 1.0 }};\n"
            )
            pairs = [(FOOD_USE, fade + "        score -= (closest_dist as f64 * panic_value(&scores.food, buffer) * fade) as i32;\n")]
        elif v == "no_cramp":
            pairs = [(CRAMP_USE, "")]
        elif v.startswith("cramp_scale_") and v[len("cramp_scale_"):].isdigit():
            pairs = [(CRAMP_WEIGHT_OLD, f"* CRAMP_WEIGHT * {int(v[len('cramp_scale_'):])}.0 / 100.0 * unit as f64)")]
        elif v.startswith("cramp_c_") and v[len("cramp_c_"):].isdigit():
            pairs = [(CRAMP_THRESH_OLD, f"const CRAMP_THRESHOLD: f64 = {int(v[len('cramp_c_'):])}.0 / 10.0;\n")]
        else:
            sys.exit(f"unknown eval variant: {v}")
        for old, new in pairs:
            if old not in text:
                sys.exit(f"variant {v} needs this text in {path}, which is not there (already applied, or the eval moved on):\n{old}")
            text = text.replace(old, new, 1)
    return text


if __name__ == "__main__":
    if len(sys.argv) != 3:
        sys.exit(__doc__)
    apply(sys.argv[1], sys.argv[2])
