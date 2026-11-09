#!/usr/bin/env python3
"""Repete no container `horizon` (H_NAME=horizon) os rastros de fluxo dos fluidos do original (`FluidsTraceTest`, `rastros/fluids_flow_*.jsonl`) e os
casos do balde (`rastros/fluids_bucket.jsonl`): o fluido de mod corre como bloco de líquido, e o balde põe e pega a fonte.

Servidor parado (`tick freeze`), um `tick step 1` por tick do rastro. O rastro grava as células que mudaram a cada 5 ticks; o jogo só diz quantas
células de um bloco há numa caixa (`horizon-ctl blocks`), então a comparação é pelo número de células de fluido em cada marca do rastro, e pelo
nível das células da linha do meio.

Uso: H_NAME=horizon h2_fluidos.py flat|waterfall|bucket|galeria
"""
import json, os, re, subprocess, sys, time

import h2_identity as h

TRACES = h.TRACES
ORIGIN = (100, -60, 60)  # o canto do volume do rastro; o chão de pedra fica em y = ORIGIN[1]
FLUID = "mekanism:brine"


def cmd(text):
    return h.command(text)


def step(n=1):
    cmd(f"tick step {n}")
    time.sleep(0.35 + 0.05 * n)


def at(x, y, z):
    return f"{ORIGIN[0] + x} {ORIGIN[1] + y} {ORIGIN[2] + z}"


def rows(name):
    return [json.loads(line) for line in open(f"{TRACES}/fluids_flow_{name}.jsonl")]


def count_in_box(size, block):
    a = at(0, 0, 0)
    b = at(size[0] - 1, size[1] - 1, size[2] - 1)
    out = json.loads(h.ctl("blocks", *a.split(), *b.split()))
    return out["counts"].get(block, 0)


def level_at(x, y, z):
    text = h.ctl("block", *at(x, y, z).split())
    found = re.search(r"level=(\d+)", text)
    return int(found.group(1)) if found and FLUID in text else None


def prepare(size, setup):
    cmd("tick unfreeze")
    cmd(f"fill {at(0, 0, 0)} {at(size[0] - 1, size[1] - 1, size[2] - 1)} minecraft:air")
    cmd(f"fill {at(0, 0, 0)} {at(size[0] - 1, 0, size[2] - 1)} minecraft:stone")
    time.sleep(1)
    cmd("tick freeze")
    time.sleep(0.3)
    setup()


def run(name, size, setup):
    trace = rows(name)
    prepare(size, setup)
    cells = {}
    ok = True
    t = 0
    for row in trace:
        target = row["tick"]
        if row.get("full"):
            for c in row["cells"]:
                cells[tuple(c[:3])] = (c[3], c[4])
            continue
        if target > t:
            step(target - t)
            t = target
        for c in row["cells"]:
            cells[tuple(c[:3])] = (c[3], c[4])
        want = sum(1 for v in cells.values() if v[0] == FLUID)
        got = count_in_box(size, FLUID)
        same = want == got
        ok &= same
        print(("OK   " if same else "DIFF ") + f"{name} t={t}: células de fluido {got} (rastro {want})")
        if not same:
            # onde as duas listas diferem, na linha do meio
            for (x, y, z), (block, level) in sorted(cells.items()):
                if block == FLUID and level_at(x, y, z) != level:
                    print(f"      {x},{y},{z}: rastro {level}, jogo {level_at(x, y, z)}")
                    break
    return ok


def flat():
    return run("flat", (31, 4, 31), lambda: cmd(f"setblock {at(15, 1, 15)} {FLUID}[level=0,up=false]"))


def waterfall():
    def setup():
        cmd(f"fill {at(10, 1, 10)} {at(14, 6, 14)} minecraft:stone")
        cmd(f"setblock {at(10, 7, 10)} {FLUID}[level=0,up=false]")

    return run("waterfall", (25, 12, 25), setup)


def main():
    what = sys.argv[1] if len(sys.argv) > 1 else "flat"
    ok = {"flat": flat, "waterfall": waterfall}[what]()
    sys.exit(0 if ok else 1)


if __name__ == "__main__":
    main()
