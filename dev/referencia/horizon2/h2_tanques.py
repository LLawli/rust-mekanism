#!/usr/bin/env python3
"""Repete no container `horizon` (H_NAME=horizon) os rastros do Fluid Tank do original (`TankTraceTest.java`, `rastros/tank_fluid_*.jsonl`).

    H_NAME=horizon h2_tanques.py CENARIO [CENARIO...]     (slots_both, slots_modes, slots_tank_item, mode_reset, tiers, light)

Põe a coluna de tanques do cenário em BASE, com o servidor parado (`tick freeze`), e a cada linha do rastro aplica as marcas como comandos de dados, anda um tick
(`tick step 1`) e lê o NBT de cada tanque: o fluido, a quantidade, o comparador (`current_redstone`), o estado ativo, o modo de edição, o atraso da luz e os dois slots
(o item, a quantidade e o que o item guarda no `mekanism:fluids`). A escala e a válvula não estão no NBT e ficam de fora. As marcas `push` e `pull` (a capability do
bloco chamada por fora) não têm comando: os cenários que as têm (`column`, `creative`) não rodam aqui. A troca de modo (`nextMode`) é um `data merge` do modo novo com a
direção do slot zerada, que é o que `editModeChanged` faz.
"""
import json, os, re, sys, time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import h2_identity as h

RASTROS = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "rastros")
BASE = (3000, -60, 3000)
MODES = ["both", "fill", "empty"]


def pos(index):
    return f"{BASE[0]} {BASE[1] + index} {BASE[2]}"


def step(ticks=1):
    h.command(f"tick step {ticks}")
    time.sleep(0.35 + 0.05 * ticks)


def bodies(text):
    depth, start, out = 0, None, []
    for i, c in enumerate(text):
        if c == "{":
            if depth == 0:
                start = i
            depth += 1
        elif c == "}":
            depth -= 1
            if depth == 0:
                out.append(text[start:i + 1])
    return out


def read(index):
    """O que o NBT do tanque diz, na forma da linha do rastro."""
    out = h.command(f"data get block {pos(index)}")[0]
    if "block data" not in out:
        return None
    data = out.split("block data: ", 1)[1]
    tank = re.search(r"tanks: \[\{([^}]*)\}\]", data)
    fluid, amount = "", 0
    if tank:
        amount = int(re.search(r"amount: (\d+)L", tank.group(1)).group(1))
        resource = re.search(r'resource: "([^"]+)"', tank.group(1))
        fluid = resource.group(1) if resource and amount > 0 else ""
    slots = {0: ["", 0, "", 0], 1: ["", 0, "", 0]}
    items = re.search(r"Items: (\[.*?\]), [a-z_]+: ", data + ", z: ")
    if "Items: [" in data:
        start = data.index("Items: [") + len("Items: ")
        depth, end = 0, start
        for i in range(start, len(data)):
            if data[i] == "[":
                depth += 1
            elif data[i] == "]":
                depth -= 1
                if depth == 0:
                    end = i
                    break
        for body in bodies(data[start + 1:end]):
            slot = int(re.search(r"Slot: (\d+)b", body).group(1))
            inside = re.search(r'"mekanism:fluids": \{containers: \[\{([^\]]*)\]', body)
            held, held_amount = "", 0
            if inside:
                held_amount = int(re.search(r"amount: (\d+)L", inside.group(1)).group(1))
                held = re.search(r'id: "([^"]+)"', inside.group(1)).group(1)
            item = re.search(r'\bid: "([^"]+)"\}?$|, id: "([^"]+)"', body)
            ids = re.findall(r'\bid: "([^"]+)"', body)
            # O id do item é o que não está dentro do componente.
            item_id = ids[-1] if not inside else [i for i in ids if i != held or ids.count(i) > 1][-1]
            if "_bucket" in item_id:
                held, held_amount = item_id.replace("_bucket", ""), 1000
            slots[slot] = [item_id, int(re.search(r"count: (\d+)", body).group(1)), held, held_amount]
    pick = lambda key, kind: int(re.search(rf"\b{key}: (-?\d+){kind}", data).group(1))
    return {
        "fluid": fluid, "amount": amount, "comparator": pick("current_redstone", ""), "active": bool(pick("active_state", "b")),
        "mode": re.search(r'edit_mode: "(\w+)"', data).group(1), "light": pick("delay", ""), "in": slots[0], "out": slots[1],
    }


def item_snbt(slot, entry):
    item, count, held, amount = entry
    comps = ""
    if item.endswith("_fluid_tank") and amount > 0:
        comps = f',components:{{"mekanism:fluids":{{containers:[{{amount:{amount}L,type:{{id:"{held}"}}}}]}}}}'
    return f'{{Slot:{slot}b,id:"{item}",count:{count}{comps}}}'


def spec_item(spec):
    part = spec.split(":")
    if part[0] == "none":
        return None
    if part[0] == "ftank":
        return [f"mekanism:{part[1]}_fluid_tank", int(part[4]), f"minecraft:{part[2]}", int(part[3])]
    return [f"minecraft:{part[0]}", int(part[1]) if len(part) > 1 else 1, "", 0]


def apply(mark, state):
    part = mark.split(" ")
    index = int(part[1])
    if part[0] == "fluid":
        tank = "{amount:0L}" if part[2] == "none" else f'{{resource:"minecraft:{part[2]}",amount:{part[3]}L}}'
        h.command(f"data merge block {pos(index)} {{machinery_basis:{{v:1,tanks:[{tank}]}}}}")
    elif part[0] in ("in", "out"):
        slots = state[index]
        slots[0 if part[0] == "in" else 1] = spec_item(part[2])
        items = ",".join(item_snbt(slot, entry) for slot, entry in enumerate(slots) if entry)
        h.command(f"data merge block {pos(index)} {{Items:[{items}]}}")
    elif part[0] == "mode":
        now = h.command(f"data get block {pos(index)} edit_mode")[0].split('"')[1]
        new = MODES[(MODES.index(now) + (1 if part[2] == "next" else -1)) % 3]
        h.command(f'data merge block {pos(index)} {{edit_mode:"{new}",last_transfer_direction:"unknown"}}')
    elif part[0] == "active":
        h.command(f"data merge block {pos(index)} {{active_state:{1 if part[2] == 'true' else 0}b}}")
    else:
        raise SystemExit(f"marca sem comando: {mark}")


def replay(name):
    lines = [json.loads(line) for line in open(os.path.join(RASTROS, f"tank_fluid_{name}.jsonl"))]
    tanks = lines[0]["meta"]["tanks"]
    h.command("tick unfreeze")
    for index in range(len(tanks) + 1):
        h.command(f"setblock {pos(index)} minecraft:air")
    for index, tank in enumerate(tanks):
        h.command(f"setblock {pos(index)} mekanism:{tank['tier']}_fluid_tank")
    time.sleep(1.0)
    h.command("tick freeze")
    time.sleep(0.5)
    step(2)
    state = [[None, None] for _ in tanks]
    diffs, compared = [], 0
    for line in lines[1:]:
        for mark in line["marks"]:
            apply(mark, state)
        step()
        for index, recorded in enumerate(line["tanks"]):
            ours = read(index)
            state[index] = [ours["in"] if ours["in"][1] else None, ours["out"] if ours["out"][1] else None]
            for key in ("fluid", "amount", "comparator", "active", "mode", "light"):
                if ours[key] != recorded[key]:
                    diffs.append((line["t"], index, key, ours[key], recorded[key]))
            for key in ("in", "out"):
                if ours[key] != recorded[key][:4]:
                    diffs.append((line["t"], index, key, ours[key], recorded[key][:4]))
            compared += 1
    h.command("tick unfreeze")
    for index in range(len(tanks) + 1):
        h.command(f"setblock {pos(index)} minecraft:air")
    print(f"{name}: {compared} estados, {len(diffs)} diferenças")
    for diff in diffs[:12]:
        print("  t={} tanque {} {}: port {} original {}".format(*diff))
    return len(diffs)


if __name__ == "__main__":
    total = sum(replay(name) for name in sys.argv[1:])
    sys.exit(1 if total else 0)
