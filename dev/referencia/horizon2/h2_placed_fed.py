#!/usr/bin/env python3
"""Repete no container `horizon` (H_NAME=horizon) o rastro `placed_fed_<máquina>.jsonl` (`PlacedTraceTest.fed`): a máquina posta por um jogador ao lado de um Heat Generator
cheio (240), de frente para ela, e a energia que ela tem depois de cada tick.

    H_NAME=horizon h2_placed_fed.py [MAQUINA...]

O original põe a capacidade do item (1.000) na colocação (`setPlacedBy` -> `applyImplicitComponents`), então o gerador esvazia os 240 na máquina no mesmo tick; com a capacidade do
bloco (20) ele esvaziaria 20 por tick. O servidor fica parado e a colocação sai entre dois ticks, como uma ação de jogador (que roda antes do tick, decisão 8 do plano): é a
fase `pre` do rastro. O item do port não carrega upgrades, então só o par (0, 0) se repete.
"""
import json, os, re, sys, time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import h2_identity as h

RASTROS = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "rastros")
BASE = (0, 150, 0)
MACHINES = ["enrichment_chamber", "crusher", "energized_smelter", "metallurgic_infuser"]
GENERATOR = "mekanismgenerators:heat_generator"


def text(p):
    return f"{p[0]} {p[1]} {p[2]}"


def step(ticks=1):
    h.command(f"tick step {ticks}")
    time.sleep(0.35 + 0.05 * ticks)


def energy(p):
    out = h.command(f"data get block {text(p)} energy")[0]
    found = re.search(r"data: (-?\d+)", out) or re.search(r"(-?\d+)$", out.strip())
    return int(found.group(1)) if found else None


def run(name):
    rows = [json.loads(l) for l in open(os.path.join(RASTROS, f"placed_fed_{name}.jsonl"))]
    want = [r for r in rows if r.get("placed") == "pre_0_0" and "after_tick" in r]
    start = next(r for r in rows if r.get("placed") == "pre_0_0" and "at_placement" in r)
    x0, y0, z0 = BASE
    machine, generator = (x0 + 3, y0, z0 + 2), (x0 + 2, y0, z0 + 2)
    block = h.BLOCK[name]
    h.ctl("close")
    h.command("gamemode creative")
    h.command("tick unfreeze")
    h.command(f"tp @s {x0 + 6} {y0 + 12} {z0 + 4}")
    h.ctl("wait", "30")
    h.command(f"fill {x0} {y0 - 1} {z0 - 2} {x0 + 12} {y0 - 1} {z0 + 8} minecraft:stone")
    h.command(f"fill {x0} {y0} {z0 - 2} {x0 + 12} {y0 + 8} {z0 + 8} minecraft:air")
    h.command("tick freeze")
    h.command(f"setblock {text(generator)} {GENERATOR}[facing=east]{{energy:240}}")
    step(3)
    h.command(f"data merge block {text(generator)} {{energy:240}}")
    # O jogador põe a máquina entre dois ticks.
    h.command("clear @s")
    h.ctl("give", block, "1")
    h.command(f"tp @s {machine[0]} {machine[1] + 3} {machine[2]}")
    out = h.ctl("place", str(machine[0]), str(machine[1]), str(machine[2]), "--item", block)
    assert '"error"' not in out, out
    wrong, compared = [], 0
    print(f"{name}: no rastro, a máquina nasce com capacidade {start['at_placement']['capacity']}")
    for line in want:
        step()
        got = (energy(generator), energy(machine))
        expect = (line["after_tick"]["generator"], line["after_tick"]["energy"])
        compared += 1
        print(f"  depois do tick {line['since']}: gerador {got[0]}, máquina {got[1]} (rastro: {expect[0]}, {expect[1]})")
        if got != expect:
            wrong.append((line["since"], got, expect))
    h.command(f"fill {x0} {y0} {z0 - 2} {x0 + 12} {y0 + 8} {z0 + 8} minecraft:air")
    print(f"{name}: {compared} leituras, {len(wrong)} diferenças")
    return 1 if wrong else 0


if __name__ == "__main__":
    names = sys.argv[1:] or MACHINES
    sys.exit(max(run(n) for n in names))
