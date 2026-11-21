#!/usr/bin/env python3
"""Eval-symmetry variants for bench-depth/prepare.sh.

    eval_variants.py <heuristics.rs> <variant>

The eval in crates/snake-ai/src/heuristics.rs scores the position from "my" side only in several places. Each variant
below removes one of those asymmetries (or, for the control, the term itself), so the arena can say whether it matters:

  sym_len        length term is (my length - enemy length) instead of my length only (found: no gain).
  no_food        control: the food term is removed (found: -55 Elo).
  no_food_dist   undo "food distance": the food term uses the straight-line distance to the nearest food again, not the
                 path distance the Voronoi flood found around bodies.
  no_food_terr   undo "food in territory": the territory term no longer counts food cells each snake reaches first.
  food_terr_<P>  the food-in-territory term at P percent of its engine weight (one territory cell per food cell is 100):
                 food_terr_200 is two cells, food_terr_50 half a cell. Needs the term to be in the engine.
  food_fade_<LO>_<HI>  the flat pull toward food fades out as the board fills: full below LO percent occupancy (both snakes' lengths
                 over the board's cells), none at HI percent and above, linear between. Only while health is comfortable (buffer over
                 40): below that the full pull stays so snakes still eat in time. Data from self-play (harness dump) says that late in
                 the game, being farther from food than the enemy goes with winning beyond what the score says.

Every replacement fails loudly if the text it expects is not in the file (the eval moved on)."""
import sys

LEN_OLD = "    let mut score = me.body.len() as i32 * scores.length;\n"
LEN_NEW = "    let mut score = (me.body.len() as i32 - enemy.body.len() as i32) * scores.length;\n"

FOOD_USE = "        score -= (closest_dist as f64 * panic_value(&scores.food, buffer)) as i32;\n"

DIST_OLD = """        // Path distance around bodies when the Voronoi flood found a food cell, else the straight-line guess.
        let closest_dist = if voronoi.my_food_turn > 0 { voronoi.my_food_turn } else { closest_dist };
"""

TERR_OLD = "    score += (voronoi.my_food - voronoi.enemy_food) * unit;\n"

FOOD_OLD = """    if grid.food.any() {
        let mut closest_dist = i32::MAX;
        let mut food = grid.food;
        while let Some(idx) = food.pop_first() {
            let cell = Point {
                x: (idx as i32) % grid.width,
                y: (idx as i32) / grid.width,
            };
            closest_dist = closest_dist.min(manhattan(my_head, cell));
        }

        let buffer = me.health - closest_dist;
        score -= (closest_dist as f64 * panic_value(&scores.food, buffer)) as i32;
    }
"""

def apply(path, variant):
    text = open(path).read()
    open(path, "w").write(apply_text(text, path, [variant]))


def apply_text(text, path, variants):
    for v in variants:
        if v == "sym_len":
            pairs = [(LEN_OLD, LEN_NEW)]
        elif v == "no_food":
            pairs = [(FOOD_OLD, "")]
        elif v == "no_food_dist":
            pairs = [(DIST_OLD, "")]
        elif v == "no_food_terr":
            pairs = [(TERR_OLD, "")]
        elif v.startswith("food_fade_") and len(v.split("_")) == 4 and all(p.isdigit() for p in v.split("_")[2:]):
            lo, hi = (int(p) for p in v.split("_")[2:])
            fade = (
                "        let occupancy = (me.body.len() + enemy.body.len()) as f64 * 100.0 / (grid.width * grid.height) as f64;\n"
                f"        let fade = if buffer > 40 {{ (({hi}.0 - occupancy) / ({hi}.0 - {lo}.0)).clamp(0.0, 1.0) }} else {{ 1.0 }};\n"
            )
            pairs = [(FOOD_USE, fade + "        score -= (closest_dist as f64 * panic_value(&scores.food, buffer) * fade) as i32;\n")]
        elif v.startswith("food_terr_") and v[len("food_terr_"):].isdigit():
            pct = int(v[len("food_terr_"):])
            pairs = [(TERR_OLD, f"    score += (voronoi.my_food - voronoi.enemy_food) * unit * {pct} / 100;\n")]
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
