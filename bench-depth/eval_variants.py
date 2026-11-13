#!/usr/bin/env python3
"""Eval-symmetry variants for bench-depth/prepare.sh.

    eval_variants.py <heuristics.rs> <variant>

The eval in crates/snake-ai/src/heuristics.rs scores the position from "my" side only in several places. Each variant
below removes one of those asymmetries (or, for the control, the term itself), so the arena can say whether it matters:

  sym_len        length term is (my length - enemy length) instead of my length only.
                 Today eating is worth `length + eat_reward` to me but only `eat_reward` to deny the enemy.
  sym_trap       both seal verdicts count and both have the same size (the mean of |trap_danger| and |enemy_trapped|).
                 Today "I am sealed" shadows "enemy is sealed", and the two have different sizes.
  sym_food       the enemy's food urgency is subtracted too, and a snake that cannot reach any food in time is a
                 hard verdict for either side (both unreachable = draw).
  sym_all        sym_len + sym_trap + sym_food (the three that are still asymmetric).
  no_food        control: the food term and its hard verdict are removed. If sym_food and no_food both do no better
                 than `none`, the food term is not earning its place either way.

Landed in the engine, so no longer variants: sym_tail = both leaving tails cleared before Voronoi; vor_len = a contested
Voronoi cell goes to the longer snake; vor_time = body segments leave the Voronoi walls one by one.

With every sym_* applied, evaluate(me, enemy) == -evaluate(enemy, me) exactly; `harness asym` checks that.
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
        let panic_value = if buffer > 0 {
            scores.food.intensity * (scores.food.threshold / (buffer as f64 + 1.0)).powf(scores.food.exponent)
        } else {
            scores.food.intensity * 100.0
        };
        score -= (closest_dist as f64 * panic_value) as i32;
    }
"""

# Same curve as the original, for either snake. Returns (cost, cannot_reach_in_time). The hard verdict (nearest food
# farther than the snake's health) almost never fires: sym_food played identically to a version without it.
FOOD_NEW = """    if grid.food.any() {
        let urgency = |head: Point, health: i32| -> (i32, bool) {
            let mut closest_dist = i32::MAX;
            let mut food = grid.food;
            while let Some(idx) = food.pop_first() {
                let cell = Point {
                    x: (idx as i32) % grid.width,
                    y: (idx as i32) / grid.width,
                };
                closest_dist = closest_dist.min(manhattan(head, cell));
            }
            let buffer = health - closest_dist;
            let panic_value = if buffer > 0 {
                scores.food.intensity * (scores.food.threshold / (buffer as f64 + 1.0)).powf(scores.food.exponent)
            } else {
                scores.food.intensity * 100.0
            };
            ((closest_dist as f64 * panic_value) as i32, closest_dist > health)
        };
        let (my_cost, my_doomed) = urgency(my_head, me.health);
        let (en_cost, en_doomed) = urgency(enemy_head, enemy.health);
@@VERDICT@@
        score += en_cost - my_cost;
    }
"""
FOOD_HARD = """        match (my_doomed, en_doomed) {
            (true, true) => return scores.draw,
            (true, false) => return scores.loss,
            (false, true) => return scores.win,
            (false, false) => {}
        }"""


def apply(path, variant):
    text = open(path).read()
    if variant == "sym_all":
        text = apply_text(text, path, ["sym_len", "sym_trap", "sym_food"])
    else:
        text = apply_text(text, path, [variant])
    open(path, "w").write(text)


def apply_text(text, path, variants):
    for v in variants:
        if v == "sym_len":
            pairs = [(LEN_OLD, LEN_NEW)]
        elif v == "sym_trap":
            pairs = [(TRAP_OLD, TRAP_NEW)]
        elif v == "sym_food":
            pairs = [(FOOD_OLD, FOOD_NEW.replace("@@VERDICT@@", FOOD_HARD))]
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
