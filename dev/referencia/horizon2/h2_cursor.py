#!/usr/bin/env python3
"""Repete no horizon2 o rastro slot_click_<máquina>.jsonl (SlotClickTraceTest.java): o clique do cursor em cada slot das quatro máquinas e
do Heat Generator, com a mão, o slot e o tanque em cada estado do Java.

A máquina é posta por um jogador (`place`), o menu abre com `interact`, a mão leva o item (um clique na barra pega a pilha), e cada
linha escolhida do rastro vira um clique esquerdo, um direito, uma tecla numérica (o cursor pousa sobre o slot e `key 1`) ou um
arrasto até o primeiro slot do inventário. O resultado (slot, cursor, barra e o slot do arrasto) é comparado com o do Java.

Uso: h2_cursor.py MAQUINA [--todas]    (máquina: enrichment_chamber, crusher, energized_smelter, metallurgic_infuser, heat_generator)
Sem `--todas` escolhe uma linha por (tanque, entrada, tipo de slot, ação, conteúdo, aceita); com ela, todas as de itens com pilha de 64.
"""
import json, os, sys, time

import sys
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import h2_identity as h

TRACES = h.TRACES
BLOCK = {**h.BLOCK, "heat_generator": "mekanismgenerators:heat_generator"}
# Onde cada máquina fica: em fila, a norte do jogador, a 3 blocos uma da outra.
SPOT = {"enrichment_chamber": (70, -60, 64), "crusher": (74, -60, 64), "energized_smelter": (78, -60, 64),
        "metallurgic_infuser": (82, -60, 64), "heat_generator": (86, -60, 64)}
# O índice de cada tipo de slot em `Items` (e no menu: 300 mais ele).
KINDS = {
    "enrichment_chamber": ["input", "output", "energy"],
    "crusher": ["input", "output", "energy"],
    "energized_smelter": ["input", "output", "energy"],
    "metallurgic_infuser": ["infusion", "input", "output", "energy"],
    "heat_generator": ["fuel", "energy"],
}
MEKANISM = {"bio_fuel"}
# Itens de pilha 1: o Java pôs 3 deles num só stack, que o jogo não deixa.
SINGLE = {"lava_bucket", "water_bucket", "bucket"}
CHEMICAL = {"bio": "mekanism:bio", "fungi": "mekanism:fungi"}


# O Heat Generator queima o combustível assim que a lava tem lugar, e o slot de infusão do Infuser converte o item no tanque no tick seguinte:
# com o servidor parado a cena fica como o clique a deixou, como o rastro do Java (que lê no mesmo tick). `wait` conta ticks do jogo e não anda parado, então a espera vira tempo real.
FROZEN = False


def settle(ticks):
    if FROZEN:
        time.sleep(0.12 * ticks)
    else:
        h.ctl("wait", str(ticks))


def item_id(name):
    return f"mekanism:{name}" if name in MEKANISM else f"minecraft:{name}"


def screen():
    return json.loads(h.ctl("screen"))


def slot_of(scr, number):
    for entry in scr["slots"]:
        if entry["slot"] == number:
            item = entry.get("item")
            return [item.split(":", 1)[1] if item else "", entry.get("count", 0) if item else 0]
    return None


def carried(scr):
    c = scr.get("carried")
    return [c["item"].split(":", 1)[1], c["count"]] if c else ["", 0]


def place(machine):
    x, y, z = SPOT[machine]
    h.ctl("close")
    h.ctl("tp", str(x), str(y), str(z - 3))
    h.ctl("wait", "5")
    h.command(f"setblock {x} {y} {z} minecraft:air")
    time.sleep(0.5)
    h.command("clear @s")
    h.ctl("give", BLOCK[machine], "1")
    out = h.ctl("place", str(x), str(y), str(z), "--item", BLOCK[machine])
    if "error" in json.loads(out):
        raise SystemExit(f"não pôs {machine}: {out[:200]}")
    if machine == "heat_generator":
        # O tanque cheio de lava: a máquina não queima o combustível que o clique deixa.
        h.command(f'data merge block {x} {y} {z} {{machinery_basis:{{v:1,tanks:[{{resource:"minecraft:lava",amount:1000L}}]}}}}')


def set_state(machine, row):
    x, y, z = SPOT[machine]
    kinds = KINDS[machine]
    items = {}
    slot, item, content = row["slot"], row["item"], row["content"]
    if row["input"] and kinds[slot] != "input":
        items[kinds.index("input")] = (item_id(row["input"]), 1)
    if content == "other":
        items[slot] = ("minecraft:stick", 4)
    elif content == "same":
        items[slot] = (item_id(item), 4)
    entries = ",".join(f'{{Slot:{i}b,id:"{name}",count:{count}}}' for i, (name, count) in sorted(items.items()))
    tank = row["tank"]
    tanks = f'[{{container:0b,stored:{{type:"{CHEMICAL[tank]}",amount:500L}}}}]' if tank in CHEMICAL else "[]"
    patch = f"{{Items:[{entries}]"
    if machine == "metallurgic_infuser":
        patch += f",chemical_tanks:{tanks}"
    h.command(f"data merge block {x} {y} {z} {patch}}}")
    if machine == "metallurgic_infuser" and tank not in CHEMICAL:
        h.command(f"data remove block {x} {y} {z} chemical_tanks")
    if machine == "heat_generator":
        lava = "1000L" if tank == "lava" else "0L"
        resource = 'resource:"minecraft:lava",' if tank == "lava" else ""
        h.command(f'data merge block {x} {y} {z} {{machinery_basis:{{v:1,tanks:[{{{resource}amount:{lava}}}]}}}}')


def open_menu(machine):
    x, y, z = SPOT[machine]
    h.ctl("close")
    for _ in range(4):
        h.ctl("interact", str(x), str(y), str(z))
        time.sleep(0.5)
        if '"screen":"container"' in h.ctl("screen").replace(" ", ""):
            return
    raise SystemExit("o menu não abriu")


def widgets():
    ui = json.loads(h.ctl("ui"))
    return {w["id"]: w for w in ui["widgets"] if w["kind"] == "slot"}


def center(w):
    b = w["bounds"]
    return str(int(b[0] + b[2] / 2)), str(int(b[1] + b[3] / 2))


def run_case(machine, row):
    """Devolve o que o jogo deixou: [slot, cursor, barra0, slot do arrasto]."""
    x, y, z = SPOT[machine]
    item = row["item"]
    action = row["action"]
    h.ctl("close")
    set_state(machine, row)
    h.command("clear @s")
    count = 1 if item in SINGLE else 3
    settle(3)
    open_menu(machine)
    if action == "number":
        h.command(f"item replace entity @s hotbar.0 with {item_id(item)} {count}")
    else:
        h.command(f"item replace entity @s hotbar.8 with {item_id(item)} {count * (2 if action == 'drag' else 1)}")
    time.sleep(0.4)
    target = 300 + row["slot"]
    if action in ("left", "right"):
        h.ctl("click-slot", "8")
        if action == "left":
            h.ctl("click-slot", str(target))
        else:
            h.ctl("click-slot", str(target), "right")
    elif action == "number":
        w = widgets()
        cx, cy = center(w[f"slot-{target}"])
        h.ctl("scroll-at", cx, cy, "0")
        h.ctl("key", "1", "tap")
    elif action == "drag":
        h.ctl("click-slot", "8")
        w = widgets()
        a, b = center(w[f"slot-{target}"]), center(w["slot-9"])
        h.ctl("drag-at", a[0], a[1], b[0], b[1])
    settle(2)
    time.sleep(0.3)
    scr = screen()
    return [slot_of(scr, target), carried(scr), slot_of(scr, 0), slot_of(scr, 9)]


def expected(row):
    return [row["slot_after"], row["carried"], row["hotbar0"], row["drain"]]


# O Energized Smelter tem o caminho da Enrichment Chamber e do Crusher (três slots, só as receitas mudam): com o slot vazio, os três
# cliques que o jogador faz de verdade; o arrasto, o slot cheio e o resto ficam para as outras duas.
REDUZIDO = {"energized_smelter"}


def select(rows, every, machine=None):
    chosen, seen = [], {}
    for row in rows:
        if machine in REDUZIDO and (row["content"] != "empty" or row["action"] == "drag"):
            continue
        if row["item"] in SINGLE and row["action"] != "left":
            continue
        key = (row["tank"], row["input"], row["kind"], row["action"], row["content"], row["may_place"])
        # O Infuser varre seis estados de tanque e entrada: a regra olha o estado, o clique não, então o estado do meio (bio e terra)
        # leva todos os cliques e os outros cinco só o clique esquerdo na mão e no slot vazios.
        if machine == "metallurgic_infuser" and (row["tank"], row["input"]) != ("bio", "dirt") and (row["action"], row["content"]) != ("left", "empty"):
            if not every:
                continue
        if every:
            if row["item"] in SINGLE:
                continue
            chosen.append(row)
            continue
        # Um por chave, e dois itens diferentes quando aceita: a regra olha o item.
        if seen.get(key, 0) < (2 if row["may_place"] else 1):
            seen[key] = seen.get(key, 0) + 1
            chosen.append(row)
    return chosen


def main():
    machine = sys.argv[1]
    every = "--todas" in sys.argv
    path = os.path.join(TRACES, f"slot_click_{machine}.jsonl")
    rows = [json.loads(line) for line in open(path)][1:]
    cases = select(rows, every, machine)
    global FROZEN
    h.command("tick unfreeze")
    place(machine)
    if machine in ("heat_generator", "metallurgic_infuser"):
        h.command("tick freeze")
        FROZEN = True
    bad = 0
    started = time.time()
    for n, row in enumerate(cases):
        got = run_case(machine, row)
        want = expected(row)
        if row["item"] in SINGLE:
            # Só o slot e o cursor: o Java pôs 3 baldes numa pilha.
            ok = got[0] == want[0] or (got[0][0] == want[0][0] and got[0][1] >= 1 and want[0][1] >= 1) or (got[0][0] == "" and want[0][0] == "")
        else:
            ok = got == want
        label = f"{machine} tanque={row['tank']} entrada={row['input'] or '-'} {row['kind']} {row['action']} {row['content']} {row['item']} aceita={row['may_place']}"
        if ok:
            print(f"OK   {label}", flush=True)
        else:
            bad += 1
            print(f"DIFF {label}: esperado {want}, obtido {got}", flush=True)
    h.command("tick unfreeze")
    print(f"{machine}: {len(cases)} casos, {bad} diferenças, {time.time() - started:.0f} s")
    sys.exit(1 if bad else 0)


if __name__ == "__main__":
    main()
