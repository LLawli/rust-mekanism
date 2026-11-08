#!/usr/bin/env python3
"""Repete no container `horizon` (H_NAME=horizon) os rastros do comparador das máquinas do Mekanism original (`ComparatorTraceTest.java`).

    H_NAME=horizon h2_comparador.py order|first_tick|levels MAQUINA [--dir DIRETORIO_DE_PRINTS]
    MAQUINA (só em `levels`): enrichment_chamber, crusher, energized_smelter, metallurgic_infuser, heat_generator

`order` e `first_tick` põem duas Enrichment Chambers encostadas, N (a vizinha) e S (a que tem o `redstone` velho), cada uma com um comparador a leste (`facing=west`:
a entrada vem do oeste, onde está a máquina), e repetem as operações do rastro como comandos. A linha 0 põe N antes de S, a linha 1 põe S antes de N: o
aviso do comparador de N sai dentro do tick de N (`TileEntityMekanism.tickServer`, `:645-652`), então S só já anda no mesmo tick se tica depois de N.
`levels` põe uma máquina com um comparador, muda o conteúdo (itens nos slots, lava no tanque) e confere o nível guardado e a saída do comparador a cada tick.

O servidor fica parado (`tick freeze`) e anda com `tick step 1`; as operações `pre` do rastro (antes dos block entities) e `step` do tick anterior (depois) são o mesmo
comando entre os mesmos dois ticks. Depois de cada tick lê o NBT da máquina (`active_state`, `progress`, `current_redstone`) e a saída de cada comparador
(`OutputSignal`) e compara com o rastro.
"""
import json, os, re, sys, time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import h2_identity as h

RASTROS = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "rastros")
BASE = (0, 150, 0)
STRIDE = 6
WARMUP = 3
CHAMBER = "mekanism:enrichment_chamber"
CLAY = "minecraft:clay"
SLOTS = {"enrichment_chamber": 3, "crusher": 3, "energized_smelter": 3, "metallurgic_infuser": 4, "heat_generator": 2}


def abs_pos(rel, row=0):
    """A posição no jogo de uma posição relativa do rastro (a linha `row` fica 6 blocos para o sul de cada anterior)."""
    return (BASE[0] + rel[0], BASE[1] + rel[1] - 1, BASE[2] + rel[2] + STRIDE * row)


def north(row):
    return (3, 1, 2)


def text(p):
    return f"{p[0]} {p[1]} {p[2]}"


def step(ticks=1):
    h.command(f"tick step {ticks}")
    time.sleep(0.5 + 0.05 * ticks)


def chat(command):
    """O texto que o comando devolve, ou "" se o jogo respondeu com erro (um bloco sem block entity, por exemplo)."""
    try:
        return h.command(command)[0]
    except (KeyError, IndexError):
        return ""


def read_nbt(p):
    out = chat(f"data get block {text(p)}")
    if "block data" not in out:
        return None
    pick = lambda key, kind: (m := re.search(rf"\b{key}: (-?\d+){kind}", out)) and int(m.group(1))
    return {
        "active": bool(pick("active_state", "b")) if re.search(r"active_state: \db", out) else None,
        "progress": pick("progress", ""),
        "energy": pick("energy", ""),
        "comparator": pick("current_redstone", ""),
        "redstone": bool(pick("redstone", "b")) if re.search(r"\bredstone: \db", out) else None,
    }


def read_output(p):
    out = chat(f"data get block {text(p)} OutputSignal")
    found = re.search(r"data: (-?\d+)", out) or re.search(r"(-?\d+)$", out.strip())
    return int(found.group(1)) if found else -1


def strict(flags):
    return "" if flags & 1 else " strict"


def comparator(p):
    h.command(f"setblock {text(p)} minecraft:comparator[facing=west] strict")


# ---- order e first_tick ----


def pos_of(who, row):
    z = 2 if who == "N" else 3
    return abs_pos((3, 1, STRIDE * row + z))


def source(row):
    return abs_pos((2, 1, STRIDE * row + 3))


def apply_order(op, state):
    w = op.split(" ")
    row = int(w[1])
    if w[0] == "put":
        who = w[2]
        extra = ""
        if who == "S":
            extra = '{control_type:"high"}'
        h.command(f"setblock {text(pos_of(who, row))} {CHAMBER}{extra}{strict(int(w[3]))}")
        state["fresh"][(who, row)] = True
    elif w[0] == "clear":
        h.command(f"setblock {text(pos_of(w[2], row))} minecraft:air{strict(int(w[3]))}")
    elif w[0] == "mode":
        pass  # já saiu no NBT do `setblock`, antes do primeiro tick
    elif w[0] == "fill":
        h.command(f'data merge block {text(pos_of("S", row))} {{energy:20000,Items:[{{Slot:0b,id:"{CLAY}",count:64}}]}}')
    elif w[0] == "src":
        block = "minecraft:redstone_block" if w[2] == "on" else "minecraft:air"
        h.command(f"setblock {text(source(row))} {block}{strict(int(w[3]))}")
    elif w[0] == "ninput":
        h.command(f'data merge block {text(pos_of("N", row))} {{Items:[{{Slot:0b,id:"{CLAY}",count:64}}]}}')
    elif w[0] == "nclear":
        h.command(f"data merge block {text(pos_of('N', row))} {{Items:[]}}")
    else:
        raise SystemExit(f"operação desconhecida {op}")


def run_order(name):
    lines = [json.loads(l) for l in open(os.path.join(RASTROS, f"{name}.jsonl"))]
    rows = len(lines[0]["rows"])
    prepare(rows, comparators=2)
    state = {"fresh": {}}
    wrong, compared = [], 0
    for index, line in enumerate(lines):
        t = line["t"]
        before = [op for op in (lines[index - 1]["ops"] if index > 0 else []) if op["phase"] == "step"] + [op for op in line["ops"] if op["phase"] == "pre"]
        for op in before:
            apply_order(op["op"], state)
        step()
        if t < 6:
            continue
        # Os `step` desta linha saem no começo da próxima volta; a leitura é a do fim do tick, antes deles.
        swapped = {int(op["op"].split(" ")[1]) for op in line["ops"] if op["phase"] == "step" and op["op"].split(" ")[0] in ("put", "clear")}
        for row in range(rows):
            want = line["rows"][row]
            got_s, got_n = read_nbt(pos_of("S", row)), read_nbt(pos_of("N", row))
            got = {"cn": read_output(abs_pos((4, 1, STRIDE * row + 2))), "cs": read_output(abs_pos((4, 1, STRIDE * row + 3)))}
            expect = {"cn": want["cn"], "cs": want["cs"]}
            if row not in swapped and want["s"] is not None and got_s is not None:
                got.update({"s.active": got_s["active"], "s.progress": got_s["progress"], "s.comparator": got_s["comparator"]})
                expect.update({"s.active": want["s"]["active"], "s.progress": want["s"]["progress"], "s.comparator": want["s"]["comparator"]})
            if row not in swapped and want["n"] is not None and got_n is not None:
                got["n.comparator"] = got_n["comparator"]
                expect["n.comparator"] = want["n"]["comparator"]
            compared += len(expect)
            miss = {key: (got[key], value) for key, value in expect.items() if got[key] != value}
            if miss:
                wrong.append((t, row, miss))
    return report(name, lines, compared, wrong, rows, cleanup=rows)


# ---- levels ----


def run_levels(machine):
    lines = [json.loads(l) for l in open(os.path.join(RASTROS, f"comparator_levels_{machine}.jsonl"))]
    marks = [l for l in lines if "mark" in l]
    ticks = [l for l in lines if "ops" in l]
    full = next((m for m in marks if m["mark"] == "all_full"), None)
    # O slot na lista do Java é o índice em `Items`: o Infuser tem o de infusão antes da entrada.
    shift = 1 if machine == "metallurgic_infuser" else 0
    prepare(1, comparators=1)
    block = {"heat_generator": "mekanismgenerators:heat_generator"}.get(machine, f"mekanism:{machine}")
    contents = {}
    wrong, compared = [], 0
    p = abs_pos((3, 1, 2))
    for index, line in enumerate(ticks):
        t = line["t"]
        before = [op for op in (ticks[index - 1]["ops"] if index > 0 else []) if op["phase"] == "step"]
        for op in before:
            w = op["op"].split(" ")
            if w[0] == "put":
                extra = '{control_type:"high"}' if machine == "heat_generator" else ""
                h.command(f"setblock {text(p)} {block}{extra}")
            elif w[0] in ("slot", "all", "none", "slots"):
                if w[0] == "slot":
                    contents[int(w[1]) + shift] = int(w[2])
                elif w[0] == "all":
                    contents = {i: 64 for i in range(len(full["slots"]))}
                elif w[0] == "none":
                    contents = {}
                if machine != "heat_generator":
                    items = ",".join(
                        f'{{Slot:{i}b,id:"minecraft:{full["slots"][i]["item"]}",count:{n}}}' for i, n in sorted(contents.items())
                    )
                    h.command(f"data merge block {text(p)} {{Items:[{items}]}}")
            elif w[0] == "lava":
                amount = min(int(w[1]), 1000)
                tank = f'{{resource:"minecraft:lava",amount:{amount}L}}' if amount else "{amount:0L}"
                h.command(f"data merge block {text(p)} {{machinery_basis:{{v:1,tanks:[{tank}]}}}}")
        step()
        if t < 6:
            continue
        want = line["rows"][0]
        swapped = any(op["phase"] == "step" and op["op"].split(" ")[0] == "put" for op in line["ops"])
        if want is None or swapped:
            continue
        nbt = read_nbt(p)
        if nbt is None:
            continue
        got = {"current": nbt["comparator"], "cn": read_output(abs_pos((4, 1, 2)))}
        expect = {"current": want["current"], "cn": want["cn"]}
        compared += len(expect)
        miss = {key: (got[key], value) for key, value in expect.items() if got[key] != value}
        if miss:
            wrong.append((t, 0, miss))
    return report(f"levels_{machine}", ticks, compared, wrong, 1, cleanup=1)


# ---- comum ----


def prepare(rows, comparators):
    x0, y0, z0 = BASE
    h.command("gamemode creative")
    h.command("tick unfreeze")
    h.command(f"tp @s {x0 + 6} {y0 + 12} {z0 + STRIDE * rows}")
    h.ctl("wait", "30")
    h.command(f"fill {x0} {y0 - 1} {z0 - 2} {x0 + 12} {y0 - 1} {z0 + STRIDE * rows + 4} minecraft:stone")
    h.command(f"fill {x0} {y0} {z0 - 2} {x0 + 12} {y0 + 8} {z0 + STRIDE * rows + 4} minecraft:air")
    h.command("tick freeze")
    for row in range(rows if comparators else 0):
        comparator(abs_pos((4, 1, STRIDE * row + 2)))
        if comparators == 2:
            comparator(abs_pos((4, 1, STRIDE * row + 3)))
    step(WARMUP)


def report(name, lines, compared, wrong, rows, cleanup):
    print(f"{name}: {len(lines)} ticks, {compared} leituras, {len(wrong)} diferenças")
    for item in wrong[:20]:
        print("  t=%d linha %d: %s" % item)
    x0, y0, z0 = BASE
    h.command(f"fill {x0} {y0} {z0 - 2} {x0 + 12} {y0 + 8} {z0 + STRIDE * cleanup + 4} minecraft:air")
    if compared < len(lines) // 2:
        print("leituras demais perdidas: o teste não prova nada")
        return 2
    return 1 if wrong else 0


def main():
    scenario = sys.argv[1]
    if scenario in ("order", "first_tick"):
        return run_order("comparator_order" if scenario == "order" else "comparator_first_tick")
    if scenario == "levels":
        return run_levels(sys.argv[2])
    raise SystemExit(__doc__)


if __name__ == "__main__":
    sys.exit(main())
