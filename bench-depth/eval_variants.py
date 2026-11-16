#!/usr/bin/env python3
"""Eval-symmetry variants for bench-depth/prepare.sh.

    eval_variants.py <heuristics.rs> <variant>

The eval in crates/snake-ai/src/heuristics.rs scores the position from "my" side only in several places. Each variant
below removes one of those asymmetries (or, for the control, the term itself), so the arena can say whether it matters:

  sym_len        length term is (my length - enemy length) instead of my length only (found: no gain).
  no_food        control: the food term is removed (found: -55 Elo).

Every replacement fails loudly if the text it expects is not in the file (the eval moved on)."""
import sys

LEN_OLD = "    let mut score = me.body.len() as i32 * scores.length;\n"
LEN_NEW = "    let mut score = (me.body.len() as i32 - enemy.body.len() as i32) * scores.length;\n"

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
