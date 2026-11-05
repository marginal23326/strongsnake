#!/usr/bin/env python3
"""Equal-depth strength test: two variants of the engine (prepare.sh builds them), both at the same fixed depth.

  python3 run.py play --shard K [--workers 3] [--depth 8] [--boards 11x11,16x9] [--minutes 240]
  python3 run.py summarize DIR [--cand NAME --base NAME]
  python3 run.py analyze DIR [--max-turns N]

`play` runs one worker process per core, each on its own seeds and board, until the time is up. Every seed is played twice
with the sides swapped, so a result is a whole pair. `summarize` reads every PAIR line in DIR and prints markdown tables."""
import argparse, glob, math, os, subprocess, sys

HERE = os.path.dirname(os.path.abspath(__file__))
BIN = f"{HERE}/harness/target/release/harness"
SEED_STRIDE = 100_000      # worker k plays seeds 1000 + k*SEED_STRIDE ..., so no two workers (or shards) share a seed


def play(a):
    boards = [tuple(b.split("x")) for b in a.boards.split(",")]   # a trailing "p" (11x11p) flips the start parity
    os.makedirs(a.out, exist_ok=True)
    procs = []
    for i in range(a.workers):
        k = a.shard * a.workers + i
        w, h = boards[k % len(boards)]
        out = open(f"{a.out}/w{k:03d}.txt", "w")
        cmd = [BIN, "match", str(1000 + k * SEED_STRIDE), str(a.max_pairs), w, h, str(a.max_turns),
               "cand", str(a.depth_cand or a.depth), "base", str(a.depth), str(a.minutes * 60)]
        procs.append((subprocess.Popen(cmd, stdout=out, stderr=subprocess.STDOUT), out))
    for p, out in procs:
        p.wait()
        out.close()


def elo(p):
    p = min(max(p, 1e-4), 1 - 1e-4)
    return -400 * math.log10(1 / p - 1)


class Tally:
    def __init__(self):
        self.pair_scores = []
        self.w = self.l = self.d = 0
        self.stats = [[0, 0.0, 0, 0.0], [0, 0.0, 0, 0.0]]   # moves, secs, depth sum, max secs  for A and B

    def add(self, line):
        left, ra, rb = line.split("|")[:3]
        f = left.split()
        r1, r2 = float(f[4]), float(f[5])
        self.pair_scores.append((r1 + r2) / 2)
        for r in (r1, r2):
            if r == 1.0: self.w += 1
            elif r == 0.0: self.l += 1
            else: self.d += 1
        for side, part in ((0, ra), (1, rb)):
            n, secs, dsum, mx = part.split()
            s = self.stats[side]
            s[0] += int(n); s[1] += float(secs); s[2] += int(dsum); s[3] = max(s[3], float(mx))

    def row(self, name):
        n = len(self.pair_scores)
        if n < 2:
            return f"| {name} | {2*n} | - | - | - | - | - | - | - | - | - |"
        m = sum(self.pair_scores) / n
        sd = math.sqrt(sum((x - m) ** 2 for x in self.pair_scores) / (n - 1))
        ci = 1.96 * sd / math.sqrt(n)
        lo, hi = m - ci, m + ci
        verdict = "A better" if lo > 0.5 else "B better" if hi < 0.5 else "no difference shown"
        a, b = self.stats
        ms = lambda s: 1000 * s[1] / s[0]
        dp = lambda s: s[2] / s[0]
        return (f"| {name} | {2*n} | {self.w} | {self.l} | {self.d} | {100*m:.2f}% | {100*lo:.2f}–{100*hi:.2f} | "
                f"{elo(m):+.1f} ({elo(lo):+.1f}…{elo(hi):+.1f}) | {ms(a):.1f} / {ms(b):.1f} | {dp(a):.2f} / {dp(b):.2f} | "
                f"{1000*a[3]:.0f} / {1000*b[3]:.0f} | {verdict} |")


def summarize(a):
    per_board, pooled = {}, Tally()
    files = sorted(glob.glob(f"{a.dir}/**/w*.txt", recursive=True))
    for fn in files:
        for line in open(fn):
            if not line.startswith("PAIR"):
                continue
            board = "x".join(line.split("|")[0].split()[2:4])
            per_board.setdefault(board, Tally()).add(line)
            pooled.add(line)
    print(f"# Equal-depth strength test: A = {a.cand}, B = {a.base}\n")
    print(f"{len(files)} worker files, {len(pooled.pair_scores)} seed pairs. Every seed is two games with the sides swapped; "
          "the 95% interval is over those pairs. Score is from A's side (win 1, draw ½).\n")
    head = ("| | games | W | L | D | score A | 95% CI | Elo A−B (95%) | ms/move A / B | depth A / B | slowest move ms A / B | verdict |\n"
            "|---|---:|---:|---:|---:|---:|---|---|---:|---:|---:|---|")
    print(head)
    print(pooled.row("**all boards**"))
    for board in sorted(per_board):
        print(per_board[board].row(board))
    odd = sum(1 for x in pooled.pair_scores if x != 0.5)
    print(f"\nPairs where the two sides did not split the pair 1-1: {odd} of {len(pooled.pair_scores)}. "
          "When both sides are meant to play identically (a force_* variant against `none`), this has to be 0.")


def analyze(a):
    """Where does the result come from? Per board: how the games ended, from A's side (who died and of what, or the turn
    cap), the length of both snakes at the end, and the split by which start corner A had."""
    rows = {}
    for fn in sorted(glob.glob(f"{a.dir}/**/w*.txt", recursive=True)):
        for line in open(fn):
            if not line.startswith("PAIR"):
                continue
            parts = line.split("|")
            f = parts[0].split()
            board = f"{f[2]}x{f[3]}"
            r = rows.setdefault(board, {"n": 0, "slot": [[0.0, 0], [0.0, 0]], "cap": [[0.0, 0], [0.0, 0]], "turns": 0,
                                        "end": {}, "len": [0, 0, 0], "lenwin": [0, 0, 0]})
            ends = parts[3].split() if len(parts) > 3 else [None, None]
            for slot, (res, turns) in enumerate(((float(f[4]), int(f[6])), (float(f[5]), int(f[7])))):
                capped = turns >= a.max_turns
                r["slot"][slot][0] += res; r["slot"][slot][1] += 1
                r["cap"][capped][0] += res; r["cap"][capped][1] += 1
                r["turns"] += turns; r["n"] += 1
                if ends[slot]:
                    codes, la, lb = ends[slot].split(":")
                    key = "A died: " + codes[0] if codes[0] != "-" and codes[1] == "-" else \
                          "B died: " + codes[1] if codes[1] != "-" and codes[0] == "-" else \
                          "both died" if codes != "--" else "neither (cap)"
                    r["end"][key] = r["end"].get(key, 0) + 1
                    r["len"][0] += int(la); r["len"][1] += int(lb); r["len"][2] += 1
    names = {"W": "hit the wall", "S": "starved", "B": "hit a body", "H": "lost head-on"}
    pct = lambda x: f"{100*x[0]/x[1]:.1f}% (n={x[1]})" if x[1] else "-"
    print("| board | games | mean turns | capped games | A score, ended early | A score, hit turn cap | A score as snake 0 (bottom-left start) | A score as snake 1 (top-right start) | mean final length A / B |")
    print("|---|---:|---:|---:|---:|---:|---:|---:|---:|")
    for board, r in sorted(rows.items()):
        n = r["n"]
        ln = f"{r['len'][0]/r['len'][2]:.1f} / {r['len'][1]/r['len'][2]:.1f}" if r["len"][2] else "-"
        print(f"| {board} | {n} | {r['turns']/n:.0f} | {100*r['cap'][1][1]/n:.1f}% | {pct(r['cap'][0])} | {pct(r['cap'][1])} | {pct(r['slot'][0])} | {pct(r['slot'][1])} | {ln} |")
    print("\n#### How the games ended (share of all games on the board; a death code is the loser's cause)\n")
    keys = [f"{who} died: {c}" for who in "AB" for c in "WSBH"] + ["both died", "neither (cap)"]
    label = lambda k: f"{k[0]} {names[k[-1]]}" if ": " in k else k
    print("| board | " + " | ".join(label(k) for k in keys) + " |")
    print("|---|" + "---:|" * len(keys))
    for board, r in sorted(rows.items()):
        tot = sum(r["end"].values())
        cells = [f"{100*r['end'].get(k, 0)/tot:.1f}%" if tot else "-" for k in keys]
        print(f"| {board} | " + " | ".join(cells) + " |")


def main():
    ap = argparse.ArgumentParser()
    sub = ap.add_subparsers(dest="cmd", required=True)
    p = sub.add_parser("play")
    p.add_argument("--shard", type=int, default=0)
    p.add_argument("--workers", type=int, default=3)
    p.add_argument("--depth", type=int, default=8)
    p.add_argument("--depth-cand", type=int, default=None, help="depth of side A when it should differ (equal-time tests)")
    p.add_argument("--boards", default="11x11,16x9")
    p.add_argument("--minutes", type=int, default=240, help="stop starting new pairs after this long")
    p.add_argument("--max-pairs", type=int, default=50000)
    p.add_argument("--max-turns", type=int, default=500)
    p.add_argument("--out", default=f"{HERE}/results")
    s = sub.add_parser("summarize")
    s.add_argument("dir")
    s.add_argument("--cand", default="cand")
    s.add_argument("--base", default="base")
    n = sub.add_parser("analyze")
    n.add_argument("dir")
    n.add_argument("--max-turns", type=int, default=500)
    a = ap.parse_args()
    {"play": play, "summarize": summarize, "analyze": analyze}[a.cmd](a)


if __name__ == "__main__":
    main()
