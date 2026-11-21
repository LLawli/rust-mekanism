#!/usr/bin/env python3
"""Repete no container do port (H_NAME) os rastros do Chemical Tank do original (`TankTraceTest.java`, `rastros/tank_chemical_*.jsonl`).

    H_NAME=horizon [BASE=x,y,z] h2_tanques_quimico.py CENARIO [CENARIO...]   (dump_basic, dump_advanced, dump_elite, dump_ultimate, eject, eject_two, slots; `mao`: o tanque posto e quebrado pelo jogador)

Põe o tanque do cenário em BASE virado para o norte, com a configuração padrão do item (a frente `OUTPUT`, o resto `INPUT`, ejetando), e os vizinhos do rastro em volta
(todos os lados `INPUT_OUTPUT`, sem ejetar), com o servidor parado (`tick freeze`). A cada linha do rastro aplica as marcas como comandos de dados, anda um tick
(`tick step 1`) e lê o NBT: o químico, a quantidade, o comparador (`current_redstone`), o modo de descarte, se ejeta, os lados de químico, os dois slots (o item, a
quantidade e o que o item guarda no `mekanism:chemicals`) e o que cada vizinho tem. As marcas `push`, `pull`, `break` e `place` não têm comando: os cenários que as têm
(`item`, `creative`) não rodam aqui. BASE tem de estar no ar: o script recusa montar a cena se algum bloco dela não for ar, e só usa `setblock` nesses blocos.
"""
import json, os, re, sys, time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import h2_identity as h

RASTROS = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "rastros")
BASE = tuple(int(v) for v in os.environ.get("BASE", "3000,200,3000").split(","))
OFFSET = {"north": (0, 0, -1), "south": (0, 0, 1), "west": (-1, 0, 0), "east": (1, 0, 0), "up": (0, 1, 0), "down": (0, -1, 0)}
DUMP = ["idle", "dumping_excess", "dumping"]
SIDES = ["front", "left", "right", "back", "top", "bottom"]
# `TransmissionType.VALUES`: energia 0, fluido 1, químico 2, item 3, calor 4. `config2` e `eject2` são os do químico, `config3` os dos itens.
TYPES = ["none", "input", "input_1", "input_2", "output", "output_1", "output_2", "input_output", "energy", "extra"]
CREATIVE = 9223372036854775807


def pos(face=None):
    d = OFFSET[face] if face else (0, 0, 0)
    return f"{BASE[0] + d[0]} {BASE[1] + d[1]} {BASE[2] + d[2]}"


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


def data_of(where):
    out = h.command(f"data get block {where}")[0]
    return out.split("block data: ", 1)[1] if "block data" in out else None


def chemical_of(data):
    tank = re.search(r"chemical_tanks: \[\{(.*?)\}\]", data)
    if not tank:
        return ["", 0]
    amount = int(re.search(r"amount: (\d+)L", tank.group(1)).group(1))
    kind = re.search(r'type: "([^"]+)"', tank.group(1)).group(1)
    return [kind, amount] if amount > 0 else ["", 0]


def read():
    """O que o NBT do tanque diz, na forma da linha do rastro."""
    data = data_of(pos())
    chemical, amount = chemical_of(data)
    slots = {0: ["", 0, "", 0], 1: ["", 0, "", 0]}
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
            inside = re.search(r'"mekanism:chemicals": \{containers: \[\{([^\]]*)\]', body)
            held, held_amount = "", 0
            if inside:
                held_amount = int(re.search(r"amount: (\d+)L", inside.group(1)).group(1))
                held = re.search(r'type: "([^"]+)"', inside.group(1)).group(1)
            item_id = re.findall(r'\bid: "([^"]+)"', body)[-1]
            slots[slot] = [item_id, int(re.search(r"count: (\d+)", body).group(1)), held, held_amount]
    config = re.search(r"component_config: \{(.*?)\}", data)
    sides, ejecting = ["none"] * 6, False
    if config:
        array = re.search(r"config2: \[I; ([^\]]*)\]", config.group(1))
        if array:
            sides = [TYPES[int(v)] for v in array.group(1).split(", ")]
        eject = re.search(r"eject2: (\d)b", config.group(1))
        ejecting = bool(eject and eject.group(1) == "1")
    dumping = re.search(r'dumping: "(\w+)"', data)
    return {
        "chemical": chemical, "amount": amount, "comparator": int(re.search(r"\bcurrent_redstone: (-?\d+)", data).group(1)),
        "dumping": dumping.group(1) if dumping else "idle", "ejecting": ejecting, "sides": sides, "drain": slots[0], "fill": slots[1],
    }


def chem_merge(where, name, amount, capacity):
    if name == "none":
        h.command(f"data remove block {where} chemical_tanks")
        return
    value = {-1: capacity, -90: capacity // 10 * 9 + 1}.get(amount, amount)
    h.command(f'data merge block {where} {{chemical_tanks:[{{container:0b,stored:{{type:"mekanism:{name}",amount:{value}L}}}}]}}')


def item_snbt(slot, entry):
    item, count, held, amount = entry
    comps = ""
    if item.endswith("_chemical_tank") and amount > 0:
        comps = f',components:{{"mekanism:chemicals":{{containers:[{{amount:{amount}L,type:"{held}"}}]}}}}'
    return f'{{Slot:{slot}b,id:"{item}",count:{count}{comps}}}'


def spec_item(spec):
    part = spec.split(":")
    if part[0] == "none":
        return None
    if part[0] == "ctank":
        return [f"mekanism:{part[1]}_chemical_tank", int(part[4]), f"mekanism:{part[2]}" if int(part[3]) else "", int(part[3])]
    if part[0] == "ctankfull":
        return [f"mekanism:{part[1]}_chemical_tank", int(part[3]), f"mekanism:{part[2]}", CREATIVE]
    return [f"minecraft:{part[0]}", int(part[1]) if len(part) > 1 else 1, "", 0]


def apply(mark, state, meta):
    part = mark.split(" ")
    capacity = meta["tanks"][0]["capacity"]
    if part[0] == "chem":
        chem_merge(pos(), part[1], int(part[2]) if len(part) > 2 else 0, capacity)
    elif part[0] == "recv":
        other = next(r for r in meta["receivers"] if r["face"] == part[1])
        chem_merge(pos(part[1]), part[2], int(part[3]), other["capacity"])
    elif part[0] == "dump":
        now = (h.command(f"data get block {pos()} dumping")[0].split('"') + ["", "idle"])[1]
        h.command(f'data merge block {pos()} {{dumping:"{DUMP[(DUMP.index(now) + 1) % 3]}"}}')
    elif part[0] in ("fillslot", "drainslot"):
        state[1 if part[0] == "fillslot" else 0] = spec_item(part[1])
        items = ",".join(item_snbt(slot, entry) for slot, entry in enumerate(state) if entry)
        h.command(f"data merge block {pos()} {{Items:[{items}]}}")
    elif part[0] == "eject":
        h.command(f"data merge block {pos()} {{component_config:{{eject2:{1 if part[1] == 'true' else 0}b}}}}")
    elif part[0] == "side":
        h.command(f"data modify block {pos()} component_config.config2[{SIDES.index(part[1])}] set value {TYPES.index(part[2])}")
    else:
        raise SystemExit(f"marca sem comando: {mark}")


def replay(name):
    lines = [json.loads(line) for line in open(os.path.join(RASTROS, f"tank_chemical_{name}.jsonl"))]
    meta = lines[0]["meta"]
    tank = meta["tanks"][0]
    receivers = meta.get("receivers", [])
    cells = [pos()] + [pos(r["face"]) for r in receivers]
    h.command("tick unfreeze")
    for where in cells:
        if "passed" not in " ".join(h.command(f"execute if block {where} minecraft:air")).lower():
            raise SystemExit(f"{where} não é ar: a cena não é montada")
    h.command(f"setblock {pos()} mekanism:{tank['tier']}_chemical_tank[facing=north]")
    for r in receivers:
        h.command(f"setblock {pos(r['face'])} mekanism:{r['tier']}_chemical_tank[facing=north]")
    time.sleep(1.0)
    h.command("tick freeze")
    time.sleep(0.5)
    step(2)
    # O padrão do item (`ItemBlockChemicalTank.SIDE_CONFIG`) no tanque, e nos vizinhos o que o rastro lhes deu.
    h.command(f"data merge block {pos()} {{component_config:{{config2:[I;4,1,1,1,1,1],config3:[I;4,1,1,1,1,1],eject2:1b,eject3:0b}}}}")
    for r in receivers:
        h.command(f"data merge block {pos(r['face'])} {{component_config:{{config2:[I;7,7,7,7,7,7],eject2:0b}}}}")
    step(1)
    state = [None, None]
    diffs, compared = [], 0
    for line in lines[1:]:
        for mark in line["marks"]:
            apply(mark, state, meta)
        step()
        ours = read()
        state = [ours["drain"] if ours["drain"][1] else None, ours["fill"] if ours["fill"][1] else None]
        recorded = line["tank"]
        for key in ("chemical", "amount", "comparator", "dumping", "ejecting", "sides"):
            if ours[key] != recorded[key]:
                diffs.append((line["t"], key, ours[key], recorded[key]))
        for key in ("drain", "fill"):
            theirs = [recorded[key][0], recorded[key][1], recorded[key][4], recorded[key][5]]
            if ours[key] != theirs:
                diffs.append((line["t"], key, ours[key], theirs))
        for face, theirs in line["receivers"].items():
            mine = chemical_of(data_of(pos(face)))
            if mine != theirs:
                diffs.append((line["t"], f"vizinho {face}", mine, theirs))
        compared += 1
    h.command("tick unfreeze")
    for where in cells:
        h.command(f"setblock {where} minecraft:air")
    print(f"{name}: {compared} estados, {len(diffs)} diferenças", flush=True)
    for diff in diffs[:12]:
        print("  t={} {}: port {} original {}".format(*diff), flush=True)
    return len(diffs)


def mao():
    """O tanque posto pelo jogador com um item que traz químico, e quebrado em sobrevivência: o que o tile recebe do item e o que o item solto leva."""
    x, y, z = BASE
    glass = f"{x} {y - 1} {z - 3}"
    # O jogador põe o bloco contra um vizinho sólido: um vidro debaixo do tanque.
    under = f"{x} {y - 1} {z}"
    for where in (pos(), glass, under):
        if "passed" not in " ".join(h.command(f"execute if block {where} minecraft:air")).lower():
            raise SystemExit(f"{where} não é ar: a cena não é montada")
    antes = h.command("data get entity @s Pos")[0]
    back = " ".join(v.rstrip("d") for v in re.search(r"\[(.*?)\]", antes).group(1).split(", "))
    h.command("tick unfreeze")
    h.command(f"setblock {glass} minecraft:glass")
    h.command(f"setblock {under} minecraft:glass")
    h.command("gamemode creative")
    h.command(f"tp @s {x + 0.5} {y} {z - 2.5} 0 0")
    # O comando de item do port só aceita os componentes da lista de `command_components.rs`, que não tem o `mekanism:dump_mode`: o modo de descarte do item fica sem medida aqui.
    h.command('item replace entity @s hotbar.1 with mekanism:basic_chemical_tank[mekanism:chemicals={containers:[{amount:5000L,type:"mekanism:hydrogen"}]}]')
    h.ctl("select", "2")
    out = h.ctl("place", str(x), str(y), str(z), "--item", "mekanism:basic_chemical_tank")
    time.sleep(1.5)
    data = data_of(pos()) or ""
    print("posto:", "erro" if "error" in out else "ok", data, flush=True)
    falhas = 0
    for nome, certo in (
        ("o químico do item entrou no tile", chemical_of(data) == ["mekanism:hydrogen", 5000]),
        ("o tile nasce sem descarte", 'dumping: "idle"' in data),
        ("a configuração padrão do item (a frente sai no químico e ejeta)", "config2: [I; 4, 1, 1, 1, 1, 1]" in data and "eject2: 1b" in data),
        ("os itens saem pela frente, sem ejetar", "config3: [I; 4, 1, 1, 1, 1, 1]" in data and "eject3: 0b" in data),
        ("o comparador lê o nível (5.000 de 64.000: 2)", "current_redstone: 2" in data),
    ):
        print(("ok   " if certo else "FALHA"), nome, flush=True)
        falhas += 0 if certo else 1
    # Um segundo modo e um item num slot, para o item solto os levar.
    h.command(f'data merge block {pos()} {{dumping:"dumping_excess",Items:[{{Slot:1b,id:"minecraft:stick",count:3}}]}}')
    h.command(f"data merge block {pos()} {{component_config:{{eject2:0b}}}}")
    time.sleep(0.5)
    h.command("gamemode survival")
    h.command("kill @e[type=minecraft:item]")
    h.command("give @s minecraft:diamond_pickaxe")
    h.ctl("select", "minecraft:diamond_pickaxe")
    h.ctl("break", str(x), str(y), str(z))
    time.sleep(1.5)
    item = " ".join(h.command("data get entity @e[type=minecraft:item,limit=1,sort=nearest] Item"))
    print("solto:", item, flush=True)
    amount = re.search(r'"mekanism:chemicals": \{containers: \[\{amount: (\d+)L', item)
    for nome, certo in (
        ("o item solto é o tanque", 'id: "mekanism:basic_chemical_tank"' in item),
        ("leva o químico (abaixo de 85 % o `DUMPING_EXCESS` não joga nada fora)", bool(amount) and int(amount.group(1)) == 5000),
        ("leva o modo de descarte", '"mekanism:dump_mode": "dumping_excess"' in item),
        ("leva os slots", '"mekanism:items"' in item and "minecraft:stick" in item),
        ("leva a configuração de lados", '"mekanism:side_config"' in item),
    ):
        print(("ok   " if certo else "FALHA"), nome, flush=True)
        falhas += 0 if certo else 1
    h.command("kill @e[type=minecraft:item]")
    h.command("gamemode creative")
    h.command("clear @s minecraft:diamond_pickaxe")
    h.command("clear @s mekanism:basic_chemical_tank")
    h.command(f"setblock {pos()} minecraft:air")
    h.command(f"tp @s {back}")
    h.command(f"setblock {glass} minecraft:air")
    h.command(f"setblock {under} minecraft:air")
    print(f"mao: {falhas} falhas", flush=True)
    return falhas


if __name__ == "__main__":
    total = sum(mao() if name == "mao" else replay(name) for name in sys.argv[1:])
    sys.exit(1 if total else 0)
