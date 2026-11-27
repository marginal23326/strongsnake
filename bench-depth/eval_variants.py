#!/usr/bin/env python3
"""Eval variants for bench-depth/prepare.sh.

    eval_variants.py <heuristics.rs> <variant>

Each variant below rewrites one piece of the eval in crates/snake-ai/src/heuristics.rs so the arena can say whether it matters:

  food_fade_<LO>_<HI>  the flat pull toward food fades out as the board fills: full below LO percent occupancy (both snakes' lengths
                 over the board's cells), none at HI percent and above, linear between. Only while health is comfortable (buffer over
                 40): below that the full pull stays so snakes still eat in time. Data from self-play (harness dump) says that late in
                 the game, being farther from food than the enemy goes with winning beyond what the score says.

  en_pull_<P>    the enemy gets the same food term as I do, with its sign flipped, at P percent: a hungry enemy far from food is
                 good for me (distance = its flood's first food turn, else straight-line to the nearest food; its own health buffer
                 sets the panic curve). The engine has the term for my snake only. Symmetric in what it counts, but it changes play:
                 the search will steer to keep the enemy away from food. One extra food scan when its flood found none.

  no_cramp       the room term is removed (undoes the cramp commit; arena: it is worth about +20 Elo).
  cramp_scale_<P>  the room term's weight at P percent of the engine's (cramp_scale_50 half, cramp_scale_200 double).
  cramp_sq_<P>   the room term's shortfall s counts as s + s*s*P/100 (convex: a snake with far too little room is punished more than
                 one slightly short). Self-play data: the fitted penalty for very cramped snakes is well above the engine's.
  cramp_c_<C>    the room term's threshold set to C/10 cells of territory per body segment (cramp_c_12 is 1.2; the engine uses 1.3).
                 Both are for scans on top of the landed term. Symmetric, no measurable cost.

Every replacement fails loudly if the text it expects is not in the file (the eval moved on)."""
import re
import sys

FOOD_USE = "        score -= (closest_dist as f64 * panic_value(&scores.food, buffer)) as i32;\n"

CRAMP_THRESH_OLD = "const CRAMP_THRESHOLD: f64 = 1.3;\n"
CRAMP_CLOSURE = "    let cramp = |count: i32, len: usize| (CRAMP_THRESHOLD - count as f64 / len as f64).max(0.0);\n"
CRAMP_WEIGHT_OLD = "* CRAMP_WEIGHT * unit as f64)"
CRAMP_USE = """    let cramp = |count: i32, len: usize| (CRAMP_THRESHOLD - count as f64 / len as f64).max(0.0);
    score += ((cramp(voronoi.enemy_count, enemy.body.len()) - cramp(voronoi.my_count, me.body.len())) * CRAMP_WEIGHT * unit as f64) as i32;
"""

FOOD_RACE_ANCHOR = "        // Food race: each turn sooner than the enemy my flood first reaches a food cell is worth a territory cell.\n"

EN_PULL = """        // Enemy food pull (variant): the mirror of the term above, from the enemy's side.
        let en_dist = if voronoi.en_food_turn > 0 {{
            voronoi.en_food_turn
        }} else {{
            let mut best = i32::MAX;
            let mut food = grid.food;
            while let Some(idx) = food.pop_first() {{
                let cell = Point {{ x: (idx as i32) % grid.width, y: (idx as i32) / grid.width }};
                best = best.min(manhattan(enemy_head, cell));
            }}
            best
        }};
        let en_buffer = enemy.health - en_dist;
        score += (en_dist as f64 * panic_value(&scores.food, en_buffer) * {p}.0 / 100.0) as i32;

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
        elif v.startswith("en_pull_") and v[len("en_pull_"):].isdigit():
            pairs = [(FOOD_RACE_ANCHOR, EN_PULL.format(p=int(v[len("en_pull_"):])) + FOOD_RACE_ANCHOR)]
        elif v.startswith("cramp_sq_") and v[len("cramp_sq_"):].isdigit():
            # shortfall s becomes s + s*s*P/100: the very cramped are punished more than the slightly cramped
            q = int(v[len("cramp_sq_"):])
            pairs = [(CRAMP_CLOSURE, CRAMP_CLOSURE.replace("let cramp =", "let cramp0 =") +
                      f"    let cramp = |count: i32, len: usize| {{ let s = cramp0(count, len); s + s * s * {q}.0 / 100.0 }};\n")]
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
