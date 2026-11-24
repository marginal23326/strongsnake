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
  food_terr_<P>  the food-in-territory term at P percent of a territory cell per food cell (the engine uses 300, so
                 food_terr_300 is the engine, food_terr_200 two cells, food_terr_400 four).
  food_race_<P>  adds a food-race term: how many steps nearer the closest food I am than the enemy (straight-line for both,
                 capped at 6 either way), worth P percent of a territory cell per step. food_race_25 is a quarter cell per step.
  food_race_late_<P>  the same, scaled by how full the board is: full weight once both snakes fill half the board or more.
  food_fade_<LO>_<HI>  the flat pull toward food fades out as the board fills: full below LO percent occupancy (both snakes' lengths
                 over the board's cells), none at HI percent and above, linear between. Only while health is comfortable (buffer over
                 40): below that the full pull stays so snakes still eat in time. Data from self-play (harness dump) says that late in
                 the game, being farther from food than the enemy goes with winning beyond what the score says.

  no_cramp       the room term is removed (undoes the cramp commit; arena: it is worth about +20 Elo).
  cramp_scale_<P>  the room term's weight at P percent of the engine's (cramp_scale_50 half, cramp_scale_200 double).
  cramp_c_<C>    the room term's threshold set to C/10 cells of territory per body segment (cramp_c_12 is 1.2; the engine uses 1.3).
                 Both are for scans on top of the landed term. Symmetric, no measurable cost.

Every replacement fails loudly if the text it expects is not in the file (the eval moved on)."""
import sys

LEN_OLD = "    let mut score = me.body.len() as i32 * scores.length;\n"
LEN_NEW = "    let mut score = (me.body.len() as i32 - enemy.body.len() as i32) * scores.length;\n"

RETURN_SCORE = "    score\n}"

FOOD_USE = "        score -= (closest_dist as f64 * panic_value(&scores.food, buffer)) as i32;\n"

DIST_OLD = """        // Path distance around bodies when the Voronoi flood found a food cell, else the straight-line guess.
        let closest_dist = if voronoi.my_food_turn > 0 { voronoi.my_food_turn } else { closest_dist };
"""

TERR_OLD = "    score += (voronoi.my_food - voronoi.enemy_food) * unit * 3;\n"
CRAMP_THRESH_OLD = "const CRAMP_THRESHOLD: f64 = 1.3;\n"
CRAMP_WEIGHT_OLD = "* CRAMP_WEIGHT * unit as f64)"
CRAMP_USE = """    let cramp = |count: i32, len: usize| (CRAMP_THRESHOLD - count as f64 / len as f64).max(0.0);
    score += ((cramp(voronoi.enemy_count, enemy.body.len()) - cramp(voronoi.my_count, me.body.len())) * CRAMP_WEIGHT * unit as f64) as i32;
"""

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
        elif v.startswith("food_race_") and v.split("_")[-1].isdigit() and (len(v.split("_")) == 3 or (len(v.split("_")) == 4 and v.split("_")[2] == "late")):
            parts = v.split("_")
            pct = int(parts[-1])
            scale = "((me.body.len() + enemy.body.len()) as f64 * 2.0 / (grid.width * grid.height) as f64).min(1.0)" if len(parts) == 4 else "1.0"
            race = (
                "    if grid.food.any() {\n"
                "        // Food race: steps nearer to the closest food than the enemy (straight-line for both).\n"
                "        let (mut my_near, mut en_near) = (i32::MAX, i32::MAX);\n"
                "        let mut food_cells = grid.food;\n"
                "        while let Some(idx) = food_cells.pop_first() {\n"
                "            let cell = Point { x: (idx as i32) % grid.width, y: (idx as i32) / grid.width };\n"
                "            my_near = my_near.min(manhattan(my_head, cell));\n"
                "            en_near = en_near.min(manhattan(enemy_head, cell));\n"
                "        }\n"
                "        let race = (en_near - my_near).clamp(-6, 6);\n"
                f"        score += (race as f64 * unit as f64 * {pct}.0 / 100.0 * {scale}) as i32;\n"
                "    }\n"
            )
            pairs = [(RETURN_SCORE, race + RETURN_SCORE)]
        elif v == "no_cramp":
            pairs = [(CRAMP_USE, "")]
        elif v.startswith("cramp_scale_") and v[len("cramp_scale_"):].isdigit():
            pairs = [(CRAMP_WEIGHT_OLD, f"* CRAMP_WEIGHT * {int(v[len('cramp_scale_'):])}.0 / 100.0 * unit as f64)")]
        elif v.startswith("cramp_c_") and v[len("cramp_c_"):].isdigit():
            pairs = [(CRAMP_THRESH_OLD, f"const CRAMP_THRESHOLD: f64 = {int(v[len('cramp_c_'):])}.0 / 10.0;\n")]
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
