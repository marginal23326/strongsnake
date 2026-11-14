#!/usr/bin/env python3
"""weight_variants.py <config.rs> <field> <percent>: scales one default weight of AiConfig to <percent>% of its value.

Fields: length, eat (eat_reward), terr (territory_control), fint / fthr / fexp (the food curve's intensity, threshold,
exponent), trap (trap_danger), etrap (enemy_trapped). It rewrites the literal in `AiConfig::default()` and fails loudly
if that literal is not there (the config moved on)."""
import re, sys

FIELDS = {  # name -> (regex of the line's left part, is_float)
    "length": (r"length: ", False),
    "eat": (r"eat_reward: ", False),
    "terr": (r"territory_control: ", False),
    "trap": (r"trap_danger: ", False),
    "etrap": (r"enemy_trapped: ", False),
    "fint": (r"intensity: ", True),
    "fthr": (r"threshold: ", True),
    "fexp": (r"exponent: ", True),
}

path, field, pct = sys.argv[1], sys.argv[2], float(sys.argv[3])
if field not in FIELDS:
    sys.exit(f"unknown weight `{field}`; known: {', '.join(FIELDS)}")
text = open(path).read()
default_start = text.index("impl Default for AiConfig")
head, tail = text[:default_start], text[default_start:]
left, is_float = FIELDS[field]
m = re.search(rf"({left})(-?[0-9_]+(?:\.[0-9_]+)?)(,)", tail)
if not m:
    sys.exit(f"weight `{field}` not found in AiConfig::default() of {path}")
old = float(m.group(2).replace("_", ""))
new = old * pct / 100.0
lit = f"{new:.3f}" if is_float else str(int(round(new)))
tail = tail[: m.start(2)] + lit + tail[m.end(2):]
open(path, "w").write(head + tail)
print(f"{field}: {m.group(2)} -> {lit}")
