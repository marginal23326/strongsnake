#!/usr/bin/env python3
"""Eval-symmetry variants for bench-depth/prepare.sh.

    eval_variants.py <heuristics.rs> <variant>

The eval in crates/snake-ai/src/heuristics.rs scores the position from "my" side only in several places. Each variant
below removes one of those asymmetries (or, for the control, the term itself), so the arena can say whether it matters:

  sym_len        length term is (my length - enemy length) instead of my length only (found: no gain).
  sym_trap       both seal verdicts count and both have the same size (found: no gain).
  no_food        control: the food term and its hard verdict are removed (found: -55 Elo).
  no_seal        neither "I am sealed" nor "enemy is sealed" verdict is applied, and the flood fills behind them are skipped.
  no_seal_me     only my seal verdict is dropped.      no_seal_enemy   only the enemy's seal verdict is dropped.
  no_starve      the hard verdict "nearest food is farther than my health = loss" is dropped (the food term stays; a
                 snake with no health to spare still pays the full panic price).

Landed in the engine, so no longer variants: sym_tail = both leaving tails cleared before Voronoi; vor_len = a contested
Voronoi cell goes to the longer snake; vor_time = body segments leave the Voronoi walls one by one.

Every replacement fails loudly if the text it expects is not in the file (the eval moved on)."""
import sys

LEN_OLD = "    let mut score = me.body.len() as i32 * scores.length;\n"
LEN_NEW = "    let mut score = (me.body.len() as i32 - enemy.body.len() as i32) * scores.length;\n"

TRAP_OLD = """    let i_am_sealed = voronoi.my_count < me.body.len() as i32 && is_sealed_in(grid, me, enemy);
    if i_am_sealed {
        score += scale_verdict(scores.trap_danger);
    } else if voronoi.enemy_count < enemy.body.len() as i32 && is_sealed_in(grid, enemy, me) {
        // If I am sealed in, that already decided things.
        score += scale_verdict(scores.enemy_trapped);
    }
"""
TRAP_NEW = """    let seal_verdict = (scores.trap_danger.abs() + scores.enemy_trapped.abs()) / 2;
    if voronoi.my_count < me.body.len() as i32 && is_sealed_in(grid, me, enemy) {
        score -= scale_verdict(seal_verdict);
    }
    if voronoi.enemy_count < enemy.body.len() as i32 && is_sealed_in(grid, enemy, me) {
        score += scale_verdict(seal_verdict);
    }
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

        if closest_dist > me.health {
            return scores.loss;
        }

        let buffer = me.health - closest_dist;
        score -= (closest_dist as f64 * panic_value(&scores.food, buffer)) as i32;
    }
"""

SEAL_ME_OLD = "    let i_am_sealed = voronoi.my_count < me.body.len() as i32 && is_sealed_in(grid, me, enemy);\n"
SEAL_ME_NEW = "    let i_am_sealed = false;\n"
SEAL_EN_OLD = "    } else if voronoi.enemy_count < enemy.body.len() as i32 && is_sealed_in(grid, enemy, me) {\n"
SEAL_EN_NEW = "    } else if false {\n"

STARVE_OLD = """        if closest_dist > me.health {
            return scores.loss;
        }

"""

def apply(path, variant):
    text = open(path).read()
    open(path, "w").write(apply_text(text, path, [variant]))


def apply_text(text, path, variants):
    for v in variants:
        if v == "sym_len":
            pairs = [(LEN_OLD, LEN_NEW)]
        elif v == "sym_trap":
            pairs = [(TRAP_OLD, TRAP_NEW)]
        elif v == "no_food":
            pairs = [(FOOD_OLD, "")]
        elif v == "no_seal_me":
            pairs = [(SEAL_ME_OLD, SEAL_ME_NEW)]
        elif v == "no_seal_enemy":
            pairs = [(SEAL_EN_OLD, SEAL_EN_NEW)]
        elif v == "no_seal":
            pairs = [(SEAL_ME_OLD, SEAL_ME_NEW), (SEAL_EN_OLD, SEAL_EN_NEW)]
        elif v == "no_starve":
            pairs = [(STARVE_OLD, "")]
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
