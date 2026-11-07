#!/usr/bin/env python3
"""Repete no horizon2 as fases do rastro identity_<máquina>.jsonl (hopper de cima e shift-clique) e compara o estado final.

Uso: h2_identity.py MAQUINA [hopper|quick]...   (máquina: enrichment_chamber, crusher, energized_smelter, metallurgic_infuser)
"""
import json, re, subprocess, sys, time

import os
TOOLS = os.path.dirname(os.path.abspath(__file__))
TRACES = os.path.join(TOOLS, "..", "rastros")
BLOCK = {
    "enrichment_chamber": "mekanism:enrichment_chamber",
    "crusher": "mekanism:crusher",
    "energized_smelter": "mekanism:energized_smelter",
    "metallurgic_infuser": "mekanism:metallurgic_infuser",
}
# O slot de entrada em `Items` e no menu (300 + índice no menu da máquina).
INPUT = {"enrichment_chamber": 0, "crusher": 0, "energized_smelter": 0, "metallurgic_infuser": 1}
POS = (70, -60, 60)
ABOVE = (70, -59, 60)


def ctl(*args):
    out = subprocess.run([f"{TOOLS}/ctl.sh", *args], capture_output=True, text=True, timeout=180)
    return out.stdout


def command(text):
    return json.loads(ctl("command", text))["chat"]


def comp_snbt(name, data):
    parts = []
    if name:
        parts.append(f'"minecraft:custom_name":\'"{name}"\'')
    if data:
        parts.append(f'"minecraft:custom_data":{{d:{data}}}')
    return "{" + ",".join(parts) + "}" if parts else ""


def item_snbt(slot, item, label=True):
    id_, name, data, count = item
    comps = comp_snbt(name, data)
    base = f'Slot:{slot}b,id:"minecraft:{id_}",count:{count}'
    return "{" + base + (f",components:{comps}" if comps else "") + "}"


def read_items(pos):
    """[(slot, id, name, data, count)] de `data get block`."""
    chat = command(f"data get block {pos[0]} {pos[1]} {pos[2]} Items")[0]
    if "has no" in chat or "[]" == chat.split("block data: ")[-1].strip():
        return []
    items = []
    text = chat.split("block data: ", 1)[-1]
    depth, start, bodies = 0, None, []
    for i, c in enumerate(text):
        if c == "{":
            if depth == 0:
                start = i
            depth += 1
        elif c == "}":
            depth -= 1
            if depth == 0:
                bodies.append(text[start:i + 1])
    for body in bodies:
        slot = int(re.search(r"Slot: (\d+)b", body).group(1))
        id_ = re.search(r'id: "(?:minecraft|mekanism):(\w+)"', body).group(1)
        count = int(re.search(r"count: (\d+)", body).group(1))
        name = re.search(r"custom_name\": '\"(\w*)\"'", body)
        data = re.search(r"custom_data\": \{d: (\d+)", body)
        items.append((slot, id_, name.group(1) if name else "", data.group(1) if data else "", count))
    return sorted(items)


def expected_items(entries, skip_empty=True):
    out = []
    for slot, (id_, name, data, count) in enumerate(entries):
        if count:
            out.append((slot, id_, name, data, count))
    return out


def place_machine(machine):
    command(f"setblock {POS[0]} {POS[1]} {POS[2]} minecraft:air")
    time.sleep(0.5)
    ctl("setblock", *map(str, POS), BLOCK[machine])
    time.sleep(1.5)


def hopper_phases(machine, lines):
    ok = True
    phases = []
    for line in lines:
        if line.get("phase", "").startswith("hopper_"):
            if line["t"] == 0:
                phases.append([])
            phases[-1].append(line)
    index = INPUT[machine]
    for phase in phases:
        first, last = phase[0], phase[-1]
        # Estado inicial: a entrada da máquina e o hopper.
        command(f"setblock {ABOVE[0]} {ABOVE[1]} {ABOVE[2]} minecraft:air")
        slot = first["slots"][index]
        items = f"[{item_snbt(index, slot)}]" if slot[3] else "[]"
        command(f"data merge block {POS[0]} {POS[1]} {POS[2]} {{Items:{items}}}")
        time.sleep(0.5)
        hopper = ",".join(item_snbt(i, it) for i, it in enumerate(first["hopper"]) if it[3])
        command(f"setblock {ABOVE[0]} {ABOVE[1]} {ABOVE[2]} minecraft:hopper[facing=down]{{Items:[{hopper}]}}")
        ctl("wait", "60")
        got_machine = [(s, i, n, d, c) for (s, i, n, d, c) in read_items(POS) if s == index]
        got_hopper = read_items(ABOVE)
        want_machine = [(index, *last["slots"][index])] if last["slots"][index][3] else []
        want_hopper = expected_items(last["hopper"])
        same = got_machine == want_machine and got_hopper == want_hopper
        ok &= same
        print(("OK   " if same else "DIFF ") + machine, first["phase"])
        if not same:
            print("  esperado máquina", want_machine, "hopper", want_hopper)
            print("  obtido   máquina", got_machine, "hopper", got_hopper)
    return ok


def screen_slots():
    d = json.loads(ctl("screen"))
    result = {}
    for s in d.get("slots", []):
        if s.get("item"):
            comps = s.get("components", "")
            name = re.search(r"custom_name\":'\"(\w*)\"'", comps)
            data = re.search(r"custom_data\":\{d:(\d+)", comps)
            result[s["slot"]] = (s["item"].replace("minecraft:", ""), name.group(1) if name else "", data.group(1) if data else "", s["count"])
    return result


QUICK = {
    "slot_a_player_b": ("inv", 9),
    "slot_a_player_a": ("inv", 10),
    "slot_a_player_plain": ("inv", 11),
    "empty_slot_player_b": ("inv", 12),
    "slot_data_player_data2": ("inv", 13),
    "slot_data_player_data1": ("inv", 14),
    "slot_a_out_hotbar_b_a": ("out", None),
    "slot_a_out_hotbar_b_only": ("out", None),
}


def quick_phases(machine, lines, scenes):
    ok = True
    index = INPUT[machine]
    # Sem o hopper das fases anteriores, que seguiria entregando.
    command(f"setblock {ABOVE[0]} {ABOVE[1]} {ABOVE[2]} minecraft:air")
    menu_input = 300 + index
    first_main = 6 if machine == "metallurgic_infuser" else 5
    first_hotbar = first_main + 27
    for line in lines:
        phase = line.get("phase", "")
        if not phase.startswith("quick_"):
            continue
        label = phase[len("quick_"):]
        scene = scenes[label]
        command("clear @s")
        command(f"data merge block {POS[0]} {POS[1]} {POS[2]} {{Items:[]}}")
        time.sleep(0.3)
        if scene["input"]:
            command(f"data merge block {POS[0]} {POS[1]} {POS[2]} {{Items:[{item_snbt(index, scene['input'])}]}}")
        for slot, item in scene["player"]:
            comps = comp_snbt(item[1], item[2])
            where = f"inventory.{slot - 9}" if slot >= 9 else f"hotbar.{slot}"
            spec = []
            if item[1]:
                spec.append(f"custom_name='\"{item[1]}\"'")
            if item[2]:
                spec.append(f"custom_data={{d:{item[2]}}}")
            suffix = f"[{','.join(spec)}]" if spec else ""
            command(f"item replace entity @s {where} with minecraft:{item[0]}{suffix} {item[3]}")
        ctl("interact", *map(str, POS))
        time.sleep(0.8)
        kind, slot = QUICK[label]
        click = slot if kind == "inv" else menu_input
        ctl("click-slot", str(click), "shift")
        time.sleep(0.5)
        got = screen_slots()
        want = {}
        for entry in line["menu"]:
            java, _, item = entry
            if java == first_main - 3:
                want[menu_input] = tuple(item)
            elif first_main <= java < first_hotbar:
                want[9 + java - first_main] = tuple(item)
            elif java >= first_hotbar:
                want[java - first_hotbar] = tuple(item)
        # O slot de entrada do Java é o 2 (ou 3 no Infuser).
        # Só compara o que o rastro lista (itens com algo), nos mesmos lugares.
        want_all = dict(want)
        got_cmp = {k: v for k, v in got.items() if k == menu_input or k < 36}
        same = got_cmp == want_all
        ok &= same
        print(("OK   " if same else "DIFF ") + machine, phase)
        if not same:
            print("  esperado", want_all)
            print("  obtido   ", got_cmp)
        ctl("close")
        time.sleep(0.3)
    return ok


def scenes_for(machine):
    valid = {"enrichment_chamber": "clay", "crusher": "cobblestone", "energized_smelter": "sand", "metallurgic_infuser": "dirt"}[machine]
    item = lambda name, data, count: (valid, name, data, count)
    return {
        "slot_a_player_b": {"input": item("A", "", 3), "player": [(9, item("B", "", 5))]},
        "slot_a_player_a": {"input": item("A", "", 3), "player": [(10, item("A", "", 5))]},
        "slot_a_player_plain": {"input": item("A", "", 3), "player": [(11, item("", "", 5))]},
        "empty_slot_player_b": {"input": None, "player": [(12, item("B", "", 5))]},
        "slot_data_player_data2": {"input": item("", "1", 3), "player": [(13, item("", "2", 5))]},
        "slot_data_player_data1": {"input": item("", "1", 3), "player": [(14, item("", "1", 5))]},
        "slot_a_out_hotbar_b_a": {"input": item("A", "", 8), "player": [(0, item("B", "", 10)), (1, item("A", "", 4))]},
        "slot_a_out_hotbar_b_only": {"input": item("A", "", 8), "player": [(0, item("B", "", 64))]},
    }


def main():
    machine = sys.argv[1]
    modes = sys.argv[2:] or ["hopper", "quick"]
    lines = [json.loads(l) for l in open(f"{TRACES}/identity_{machine}.jsonl")]
    place_machine(machine)
    ok = True
    if "hopper" in modes:
        ok &= hopper_phases(machine, lines)
    if "quick" in modes:
        ok &= quick_phases(machine, lines, scenes_for(machine))
    print("TUDO IGUAL" if ok else "HÁ DIFERENÇAS")
    sys.exit(0 if ok else 1)


if __name__ == "__main__":
    main()
