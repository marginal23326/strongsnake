#!/usr/bin/env python3
"""growth_variants.py <snake-ai/src dir> <variant>: eval variants that make the engine value food it cannot reach inside
its search horizon.

  vor_food_<pct>  Every food cell a snake's Voronoi front claims adds (1 / the turn it claimed it) cells' worth of
                  territory to that snake, scaled by <pct>%: food that is 2 steps away is worth 50 * pct / 100 percent of
                  one Voronoi cell (territory_control), food 10 steps away 10 * pct / 100 percent. The difference
                  (mine - enemy's) is added to the score. The existing food term only pulls toward the nearest food and
                  mostly when hungry; at full health it is worth about 0.05 of a cell per step. In the engine, the food
                  that piles up on the board (9.6 pieces at turn 100 of self-play, while both snakes are about 6.5 long)
                  is worth nothing until it is inside the search horizon.

  eat_urgency_<pct>  An eat counts <pct>% of eat_reward more for every ply the search still has to go below it: eating now beats eating
                  later by (plies difference) * pct% of eat_reward. Without it, eating at the first ply and at the last
                  scores the same inside the horizon, so food within reach that nobody else wants is put off for ever:
                  in self-play, with food within 3 steps, no enemy near and 70+ health, the engine steps toward it in
                  only 43% of the positions and eats it in 14%.

  eat_phase_<occ>  The eat reward fades out as the board fills: full while both snakes together fill under <occ>/2 percent of the
                  board, linearly less up to <occ> percent, none beyond. Late in a game each snake sits in its own region,
                  and eating there shrinks the region (a longer body, a tail that stays), so the reward there is wrong.
                  Combines with eat_urgency_<pct>.

Every replacement fails loudly if the text it expects is not in the file (the engine moved on)."""
import sys

def swap(path, old, new):
    text = open(path).read()
    if old not in text:
        sys.exit(f"variant needs this text in {path}, which is not there:\n{old}")
    open(path, "w").write(text.replace(old, new, 1))

src, variant = sys.argv[1], sys.argv[2]
if variant.startswith("vor_food_"):
    pct = int(variant[len("vor_food_"):])
    vor, heu = f"{src}/voronoi.rs", f"{src}/heuristics.rs"
    swap(vor, "    pub enemy_count: i32,\n}", "    pub enemy_count: i32,\n    /// Sum over the food cells each front claimed of 1000 / (the turn it claimed them).\n    pub my_food: i32,\n    pub enemy_food: i32,\n}")
    swap(vor, "    let mut turn = 0usize;\n    loop {\n        turn += 1;\n", "    let (mut my_food, mut en_food) = (0i32, 0i32);\n    let mut turn = 0usize;\n    loop {\n        turn += 1;\n")
    swap(vor, "        my_territory |= my_front;\n        en_territory |= en_front;\n",
         "        my_territory |= my_front;\n        en_territory |= en_front;\n"
         "        let (my_eatable, en_eatable) = (my_front & grid.food, en_front & grid.food);\n"
         "        if my_eatable.any() {\n            my_food += my_eatable.count_ones() as i32 * (1000 / turn as i32);\n        }\n"
         "        if en_eatable.any() {\n            en_food += en_eatable.count_ones() as i32 * (1000 / turn as i32);\n        }\n")
    swap(vor, "        enemy_count: en_territory.count_ones() as i32,\n", "        enemy_count: en_territory.count_ones() as i32,\n        my_food,\n        enemy_food: en_food,\n")
    swap(heu, "    score += (voronoi.my_count - voronoi.enemy_count) * unit;\n",
         "    score += (voronoi.my_count - voronoi.enemy_count) * unit;\n"
         f"    score += ((voronoi.my_food - voronoi.enemy_food) as i64 * unit as i64 * {pct} / 100_000) as i32;\n")
elif variant.startswith("eat_urgency_") or variant.startswith("eat_phase_"):
    path = f"{src}/search.rs"
    text = open(path).read()
    if "// @reward-adjust" not in text:  # first eat variant: give eat_bonus the depth and the board occupancy
        swap(path, "fn eat_bonus(my_ate: bool, en_ate: bool, cfg: &AiConfig) -> i32 {\n    (my_ate as i32 - en_ate as i32) * cfg.scores.eat_reward\n}",
             "fn eat_bonus(my_ate: bool, en_ate: bool, depth: usize, occupancy: i64, cfg: &AiConfig) -> i32 {\n"
             "    let mut reward = cfg.scores.eat_reward as i64;\n    // @reward-adjust\n"
             "    let _ = (depth, occupancy);\n    (my_ate as i32 - en_ate as i32) * reward as i32\n}")
        swap(path, "                let bonus = eat_bonus(my_ate, grid.food.get(grid.idx(reply.x, reply.y)), cfg);\n                let (hash, undo) = make_turn(grid, me, enemy, query.my_mv",
             "                let occupancy = ((me.body.len() + enemy.body.len()) * 100 / (grid.width * grid.height) as usize) as i64;\n"
             "                let bonus = eat_bonus(my_ate, grid.food.get(grid.idx(reply.x, reply.y)), query.node_depth, occupancy, cfg);\n                let (hash, undo) = make_turn(grid, me, enemy, query.my_mv")
        swap(path, "                    let bonus = eat_bonus(my_ate, grid.food.get(grid.idx(reply.x, reply.y)), cfg);\n                    let (hash, undo) = make_turn(grid, me, enemy, mv, reply, ctx.zobrist, root_hash);",
             "                    let occupancy = ((me.body.len() + enemy.body.len()) * 100 / (grid.width * grid.height) as usize) as i64;\n"
             "                    let bonus = eat_bonus(my_ate, grid.food.get(grid.idx(reply.x, reply.y)), depth, occupancy, cfg);\n                    let (hash, undo) = make_turn(grid, me, enemy, mv, reply, ctx.zobrist, root_hash);")
    if variant.startswith("eat_urgency_"):
        pct = int(variant[len("eat_urgency_"):])
        swap(path, "    // @reward-adjust\n", f"    reward = reward * (100 + {pct} * depth as i64) / 100;\n    // @reward-adjust\n")
    else:  # eat_phase_<occ>: full reward while both snakes fill under half of <occ>% of the board, none from <occ>% on
        occ = int(variant[len("eat_phase_"):])
        swap(path, "    // @reward-adjust\n", f"    reward = reward * (({occ} - occupancy).clamp(0, {occ} / 2)) / ({occ} / 2);\n    // @reward-adjust\n")
else:
    sys.exit(f"unknown growth variant: {variant}")
