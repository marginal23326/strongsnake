#!/usr/bin/env python3
"""Cost history of the engine, one regression-suite run per commit (used by .github/workflows/commit-cost.yml).

    commit_cost.py row <suite output file> <sha> <date> <subject> <row.json>   parse one run into a JSON row
    commit_cost.py table <dir with row.json files, in commit order>            print the markdown tables

The suite output is `snake-app test --depths ... --quiet-fail-only`, whose "Depth N summary:" lines carry
pass / fail / skipped / time / nodes / nps. Node counts are deterministic with --threads 1; time and nps are not,
so read nodes first and treat time as +-10% on a shared runner."""
import json
import re
import sys
from pathlib import Path

LINE = re.compile(
    r"Depth (\d+) summary: pass=(\d+) fail=(\d+) skipped=(\d+) time=([\d.]+)\s*(ms|s|µs|us|ns)? nodes=(\d+) nps=(\d+)"
)
UNIT_MS = {"ms": 1.0, "s": 1000.0, "µs": 1e-3, "us": 1e-3, "ns": 1e-6, None: 1.0}


def row(out_file, sha, date, subject, row_json):
    text = Path(out_file).read_text(errors="replace") if Path(out_file).exists() else ""
    depths = {}
    for m in LINE.finditer(text):
        d, p, f, s, t, unit, n, nps = m.groups()
        depths[int(d)] = dict(
            passed=int(p), failed=int(f), skipped=int(s), ms=float(t) * UNIT_MS[unit], nodes=int(n), nps=int(nps)
        )
    Path(row_json).write_text(json.dumps(dict(sha=sha, date=date, subject=subject, depths=depths)))


def fmt_nodes(n):
    return f"{n / 1e6:.2f}M" if n >= 1e6 else f"{n / 1e3:.0f}k" if n >= 1e3 else str(n)


def table(directory):
    rows = [json.loads(p.read_text()) for p in sorted(Path(directory).glob("*.json"))]
    depths = sorted({d for r in rows for d in map(int, r["depths"])})
    if not depths:
        print("No commit produced a summary line; see the job logs.")
        return
    first = next((r for r in rows if r["depths"]), None)
    for title, key, fmt in (
        ("Nodes", "nodes", fmt_nodes),
        ("Time (ms, whole suite; +-10% on a shared runner)", "ms", lambda v: f"{v:,.0f}"),
        ("Nodes per second", "nps", lambda v: f"{v / 1e6:.2f}M"),
    ):
        print(f"\n### {title}, oldest commit first\n")
        print("| commit | date | change | " + " | ".join(f"d{d}" for d in depths) + " |")
        print("|---|---|---|" + "---:|" * len(depths))
        for r in rows:
            cells = []
            for d in depths:
                x = r["depths"].get(str(d))
                cells.append(fmt(x[key]) if x else "-")
            print(f"| `{r['sha']}` | {r['date']} | {r['subject'][:48]} | " + " | ".join(cells) + " |")
    print("\n### Scenarios passed / failed / skipped (the scenario set is the newest one for every commit)\n")
    print("| commit | " + " | ".join(f"d{d}" for d in depths) + " |")
    print("|---|" + "---:|" * len(depths))
    for r in rows:
        cells = []
        for d in depths:
            x = r["depths"].get(str(d))
            cells.append(f"{x['passed']} / {x['failed']} / {x['skipped']}" if x else "-")
        print(f"| `{r['sha']}` | " + " | ".join(cells) + " |")
    if first:
        print(
            "\nNote: `skipped` counts scenarios an older commit could not load. `c114ebf` changed what a depth number "
            "means (\"treat depth as full turns\"), so compare commits on either side of it by cost, not by depth label."
        )


if __name__ == "__main__":
    if len(sys.argv) >= 2 and sys.argv[1] == "row" and len(sys.argv) == 7:
        row(*sys.argv[2:])
    elif len(sys.argv) == 3 and sys.argv[1] == "table":
        table(sys.argv[2])
    else:
        sys.exit(__doc__)
