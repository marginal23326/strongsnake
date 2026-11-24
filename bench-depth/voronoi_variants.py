#!/usr/bin/env python3
"""Graded-Voronoi variants for bench-depth/prepare.sh.

    voronoi_variants.py <crates/snake-ai/src> <variant>

The Voronoi count gives every cell a snake reaches first the same value, however far away it is. These variants count a
cell (or a food cell) the snake gets to on turn t at 1 + (E/4) * max(0, H - (t - 1)) / H cells instead: E quarter-cells
extra for a cell next to the head, fading linearly to 1 cell at turn H + 1 and beyond. E = 0 is the engine.

  terr_grad_<E>_<H>   the territory term is graded this way (food in territory stays as it is).
  food_grad_<E>_<H>   the food-in-territory term is graded this way (the territory count stays as it is).

E.g. terr_grad_4_8: a cell next to the head counts 2 cells, one 4 turns away 1.5, one 8 or more turns away 1.
Every replacement fails loudly if the text it expects is not in the file (the engine moved on)."""
import os
import sys

STRUCT_OLD = "    pub my_food: i32,\n    pub enemy_food: i32,\n}"
STRUCT_NEW = """    pub my_food: i32,
    pub enemy_food: i32,
    /// Cells and food cells weighted by how soon each snake gets there, in quarter cells.
    pub my_wterr: i32,
    pub en_wterr: i32,
    pub my_wfood: i32,
    pub en_wfood: i32,
}"""
INIT_OLD = "    let mut turn = 0usize;\n"
INIT_NEW = INIT_OLD + "    let (mut my_wterr, mut en_wterr, mut my_wfood, mut en_wfood) = (0i32, 0i32, 0i32, 0i32);\n"
ACC_OLD = "        my_territory |= my_front;\n        en_territory |= en_front;\n"
RESULT_OLD = "        enemy_food: (en_territory & grid.food).count_ones() as i32,\n    }"
RESULT_NEW = """        enemy_food: (en_territory & grid.food).count_ones() as i32,
        my_wterr,
        en_wterr,
        my_wfood,
        en_wfood,
    }"""
TERR_OLD = "    score += (voronoi.my_count - voronoi.enemy_count) * unit;\n"
FOOD_OLD = "    score += (voronoi.my_food - voronoi.enemy_food) * unit * 3;\n"


def edit(path, pairs, variant):
    text = open(path).read()
    for old, new in pairs:
        if old not in text:
            sys.exit(f"variant {variant} needs this text in {path}, which is not there (already applied, or the engine moved on):\n{old}")
        text = text.replace(old, new, 1)
    open(path, "w").write(text)


def apply(src, variant):
    parts = variant.split("_")
    if len(parts) != 4 or parts[0] not in ("terr", "food") or parts[1] != "grad" or not (parts[2].isdigit() and parts[3].isdigit() and int(parts[3]) > 0):
        sys.exit(f"unknown voronoi variant: {variant}")
    kind, extra, horizon = parts[0], int(parts[2]), int(parts[3])
    # Only cells new to a snake count: a freed tail can be claimed twice, and the plain count ORs them.
    acc = (
        "        let (my_add, en_add) = (my_front & !my_territory, en_front & !en_territory);\n"
        + ACC_OLD
        + f"        let w = 4 + {extra} * ({horizon} - (turn as i32 - 1).min({horizon})) / {horizon};\n"
        "        my_wterr += my_add.count_ones() as i32 * w;\n"
        "        en_wterr += en_add.count_ones() as i32 * w;\n"
        "        my_wfood += (my_add & grid.food).count_ones() as i32 * w;\n"
        "        en_wfood += (en_add & grid.food).count_ones() as i32 * w;\n"
    )
    edit(os.path.join(src, "voronoi.rs"), [(STRUCT_OLD, STRUCT_NEW), (INIT_OLD, INIT_NEW), (ACC_OLD, acc), (RESULT_OLD, RESULT_NEW)], variant)
    if kind == "terr":
        pair = (TERR_OLD, "    score += (voronoi.my_wterr - voronoi.en_wterr) * unit / 4;\n")
    else:
        pair = (FOOD_OLD, "    score += (voronoi.my_wfood - voronoi.en_wfood) * unit * 3 / 4;\n")
    edit(os.path.join(src, "heuristics.rs"), [pair], variant)


if __name__ == "__main__":
    if len(sys.argv) != 3:
        sys.exit(__doc__)
    apply(sys.argv[1], sys.argv[2])
