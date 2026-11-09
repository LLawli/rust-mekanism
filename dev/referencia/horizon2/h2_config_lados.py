#!/usr/bin/env python3
"""A configuração de lados por instância, no jogo (`H_NAME=horizon`, mundo plano), contra o que o original faz.

Etapas, uma por chamada (o servidor fica parado em `tick freeze` e anda por `tick step`):
  eject     um Crusher virado para cada um dos 4 lados, com um baú em cada face: a saída em OUTPUT só na direita sai só pela direita do bloco
            (a face do mundo gira com o `facing`); depois três lados de saída; INPUT_OUTPUT na frente (a entrada sai junto); sem DataType; ejetor desligado
  hopper    um hopper em cima e outro embaixo do Crusher: só entra e só sai pelo lado em que o DataType deixa
  cable     um cabo entre o Heat Generator e a máquina: a energia só entra pela face em ENERGY/INPUT
  place     o item posto por um jogador traz o padrão (o Crusher com a direita em OUTPUT, as costas em ENERGY); quebrar e repor guarda a configuração
  setblock  a máquina posta por `setblock` não aceita nem entrega por face nenhuma (tudo em NONE)
  all       todas, na ordem acima

Cada linha impressa começa com OK ou DIFF. A ordem dos lados no NBT é a de `RelativeSide` (frente, esquerda, direita, costas, alto, base) e o
DataType é o `ordinal` (0 NONE, 1 INPUT, 4 OUTPUT, 7 INPUT_OUTPUT, 8 ENERGY, 9 EXTRA).
"""
import json, os, re, subprocess, sys, time

TOOLS = os.path.dirname(os.path.abspath(__file__))
FACES = {"north": (0, 0, -1), "south": (0, 0, 1), "west": (-1, 0, 0), "east": (1, 0, 0), "down": (0, -1, 0), "up": (0, 1, 0)}
# A direita de cada frente (`RelativeSide.RIGHT`, o `getCounterClockWise`), a esquerda e as costas.
RIGHT = {"north": "west", "west": "south", "south": "east", "east": "north"}
LEFT = {value: key for key, value in RIGHT.items()}
BACK = {"north": "south", "south": "north", "west": "east", "east": "west"}
MACHINE = "mekanism:crusher"
ITEM = "minecraft:cobblestone"
GRAVEL = "minecraft:gravel"
X0, Y, Z0 = 70, -60, 60


def ctl(*args):
    out = subprocess.run([f"{TOOLS}/ctl.sh", *args], capture_output=True, text=True, timeout=180)
    return out.stdout


def command(text):
    return json.loads(ctl("command", text))["chat"]


def step(ticks):
    """`tick step N` volta antes de os N ticks andarem, a 20 por segundo (medido): espere o tempo deles."""
    command(f"tick step {ticks}")
    time.sleep(0.35 + 0.05 * ticks)


def at(origin, face):
    return tuple(o + d for o, d in zip(origin, FACES[face]))


def setblock(pos, block):
    command(f"setblock {pos[0]} {pos[1]} {pos[2]} {block}")


def read_int(pos, key):
    chat = command(f"data get block {pos[0]} {pos[1]} {pos[2]} {key}")[0]
    found = re.search(r"data: (-?\d+)", chat) or re.search(r"(-?\d+)[bBsSfFdD]?$", chat)
    return int(found.group(1)) if found else None


def read_items(pos):
    """{slot: (id, count)} de `Items`."""
    chat = command(f"data get block {pos[0]} {pos[1]} {pos[2]} Items")[0]
    found = re.findall(r'count: (\d+), Slot: (\d+)b, id: "([\w:]+)"', chat) or [
        (c, s, i) for (s, i, c) in re.findall(r'Slot: (\d+)b, id: "([\w:]+)", count: (\d+)', chat)
    ]
    return {int(slot): (item.replace("minecraft:", ""), int(count)) for count, slot, item in found}


def total(pos, item):
    name = item.replace("minecraft:", "")
    return sum(count for (found, count) in read_items(pos).values() if found == name)


def config_nbt(item=None, energy=None, eject=False):
    """`component_config` com os seis lados de cada tipo (3 itens, 0 energia)."""
    parts = []
    if energy is not None:
        parts.append("config0:[I;" + ",".join(map(str, energy)) + "]")
    if item is not None:
        parts.append("config3:[I;" + ",".join(map(str, item)) + "]")
    parts.append(f"eject3:{1 if eject else 0}b")
    return "component_config:{" + ",".join(parts) + "}"


def report(name, want, got):
    same = want == got
    print(("OK   " if same else "DIFF ") + name + ("" if same else f"  esperado {want}  obtido {got}"))
    return same


def sides_with(config_by_world):
    """Os seis lados relativos de uma frente a partir de {rel: DataType}."""
    order = ["front", "left", "right", "back", "top", "bottom"]
    return [config_by_world.get(side, 0) for side in order]


def eject():
    ok = True
    command("tick freeze")
    scenes = [
        # (nome, lados em DataType por lado relativo, itens no slot de entrada, ejetando, onde sai: lados relativos)
        ("direita", {"right": 4}, 0, True, ["right"]),
        ("tres_lados", {"right": 4, "back": 4, "bottom": 4}, 0, True, ["right", "back", "bottom"]),
        ("entrada_e_saida_na_frente", {"front": 7}, 20, True, ["front"]),
        ("sem_datatype", {}, 0, True, []),
        ("desligado", {"right": 4}, 0, False, []),
    ]
    rel_face = lambda facing, rel: {
        "front": facing, "right": RIGHT.get(facing), "left": LEFT.get(facing), "back": BACK.get(facing), "top": "up", "bottom": "down",
    }[rel]
    for name, config, entrada, ejecting, saem in scenes:
        for index, facing in enumerate(["north", "east", "south", "west"]):
            origin = (X0 + 6 * index, Y, Z0)
            setblock(origin, "minecraft:air")
            for face in FACES:
                setblock(at(origin, face), "minecraft:chest")
            setblock(origin, f"{MACHINE}[facing={facing}]")
            time.sleep(0.6)
            items = f'{{Slot:1b,id:"{GRAVEL}",count:20}}'
            if entrada:
                items += f',{{Slot:0b,id:"{ITEM}",count:{entrada}}}'
            command(
                f"data merge block {origin[0]} {origin[1]} {origin[2]} {{Items:[{items}],"
                + config_nbt(item=sides_with(config), eject=ejecting)
                + "}"
            )
            step(60)
            want = {face: 0 for face in FACES}
            for rel in saem:
                want[rel_face(facing, rel)] = 1  # "algo saiu por aqui"
            got = {face: 1 if total(at(origin, face), GRAVEL) + total(at(origin, face), ITEM) > 0 else 0 for face in FACES}
            ok &= report(f"eject {name} {facing}", want, got)
    return ok


def hopper():
    ok = True
    command("tick freeze")
    origin = (X0, Y, Z0 + 12)
    for name, top, esperado in (("entrada em cima", 1, True), ("sem DataType em cima", 0, False), ("saida em cima", 4, False)):
        setblock(origin, "minecraft:air")
        setblock(at(origin, "up"), "minecraft:air")
        setblock(origin, f"{MACHINE}[facing=north]")
        time.sleep(0.6)
        command(
            f"data merge block {origin[0]} {origin[1]} {origin[2]} {{Items:[]," + config_nbt(item=sides_with({"top": top})) + "}"
        )
        setblock(at(origin, "up"), f'minecraft:hopper[facing=down]{{Items:[{{Slot:0b,id:"{ITEM}",count:10}}]}}')
        step(60)
        got = total(origin, ITEM) > 0
        ok &= report(f"hopper de cima, {name}", esperado, got)
    for name, bottom, esperado in (("saida embaixo", 4, True), ("entrada embaixo", 1, False), ("sem DataType embaixo", 0, False)):
        setblock(origin, "minecraft:air")
        setblock(at(origin, "up"), "minecraft:air")
        under = at(origin, "down")
        setblock(under, "minecraft:hopper[facing=south]")
        setblock(origin, f"{MACHINE}[facing=north]")
        time.sleep(0.6)
        command(
            f'data merge block {origin[0]} {origin[1]} {origin[2]} {{Items:[{{Slot:1b,id:"{GRAVEL}",count:10}}],'
            + config_nbt(item=sides_with({"bottom": bottom}))
            + "}"
        )
        step(60)
        got = total(under, GRAVEL) > 0
        ok &= report(f"hopper de baixo, {name}", esperado, got)
    return ok


def cable():
    ok = True
    command("tick freeze")
    origin = (X0, Y, Z0 + 20)
    generator = at(at(origin, "east"), "east")
    cable_pos = at(origin, "east")
    for name, energy, esperado in (("energia na direita? (norte: oeste)", None, False), ("energia em todos", [1] * 6, True)):
        for pos in (origin, cable_pos, generator):
            setblock(pos, "minecraft:air")
        setblock(origin, f"{MACHINE}[facing=north]")
        setblock(cable_pos, "mekanism:basic_universal_cable")
        setblock(generator, "mekanismgenerators:heat_generator[facing=west]")
        time.sleep(0.8)
        command(
            f"data merge block {origin[0]} {origin[1]} {origin[2]} {{Items:[]," + config_nbt(item=sides_with({}), energy=energy or [0] * 6) + "}"
        )
        command(f'data merge block {generator[0]} {generator[1]} {generator[2]} {{Items:[{{Slot:0b,id:"minecraft:coal",count:64}}]}}')
        step(120)
        got = (read_int(origin, "energy") or 0) > 0
        ok &= report(f"cabo, {name}", esperado, got)
    return ok


def setblock_case():
    ok = True
    command("tick freeze")
    origin = (X0, Y, Z0 + 28)
    setblock(origin, "minecraft:air")
    setblock(origin, f"{MACHINE}[facing=north]")
    generator = at(origin, "east")
    setblock(generator, "mekanismgenerators:heat_generator[facing=west]")
    time.sleep(0.8)
    command(f'data merge block {generator[0]} {generator[1]} {generator[2]} {{Items:[{{Slot:0b,id:"minecraft:coal",count:64}}]}}')
    step(120)
    ok &= report("setblock: nenhuma face aceita energia", 0, read_int(origin, "energy") or 0)
    chat = command(f"data get block {origin[0]} {origin[1]} {origin[2]} component_config")[0]
    ok &= report("setblock: component_config tem tudo em NONE", True, "config3: [I; 0, 0, 0, 0, 0, 0]" in chat)
    return ok


def place():
    ok = True
    command("tick unfreeze")
    origin = (X0, Y, Z0 + 36)
    for pos in (origin, at(origin, "up")):
        setblock(pos, "minecraft:air")
    ctl("close")
    ctl("tp", str(origin[0]), str(origin[1]), str(origin[2] - 3))
    ctl("wait", "5")
    command("clear @s")
    ctl("give", MACHINE, "1")
    out = json.loads(ctl("place", *map(str, origin), "--item", MACHINE))
    ok &= report("place: o jogador pôs", True, "error" not in out)
    ctl("wait", "10")
    chat = command(f"data get block {origin[0]} {origin[1]} {origin[2]} component_config")[0]
    # MACHINE: tudo INPUT, direita OUTPUT, costas ENERGY. Virado para o jogador, ou para onde a guinada mandar: lê os seis valores.
    found = re.search(r"config3: \[I; ([-\d, ]+)\]", chat)
    values = [int(v) for v in found.group(1).split(",")] if found else None
    ok &= report("place: o padrão do item (frente INPUT, direita OUTPUT, costas ENERGY)", [1, 1, 4, 8, 1, 1], values)
    # Muda a configuração, quebra, pega o item e repõe.
    command(f"data merge block {origin[0]} {origin[1]} {origin[2]} {{component_config:{{config3:[I;4,0,0,7,0,0],eject3:1b}}}}")
    ctl("wait", "10")
    command(f"setblock {origin[0]} {origin[1]} {origin[2]} minecraft:air destroy")
    ctl("wait", "20")
    ctl("tp", str(origin[0]), str(origin[1]), str(origin[2]))
    ctl("wait", "20")
    out = json.loads(ctl("place", *map(str, origin), "--item", MACHINE))
    ctl("wait", "10")
    chat = command(f"data get block {origin[0]} {origin[1]} {origin[2]} component_config")[0]
    found = re.search(r"config3: \[I; ([-\d, ]+)\]", chat)
    values = [int(v) for v in found.group(1).split(",")] if found else None
    ok &= report("place: quebrar e repor guarda a configuração", [4, 0, 0, 7, 0, 0], values)
    ok &= report("place: e o ejetar", True, "eject3: 1b" in chat)
    return ok


STAGES = {"eject": eject, "hopper": hopper, "cable": cable, "setblock": setblock_case, "place": place}


def main():
    names = sys.argv[1:] or ["all"]
    if names == ["all"]:
        names = list(STAGES)
    ok = True
    for name in names:
        ok &= STAGES[name]()
    print("TUDO IGUAL" if ok else "HÁ DIFERENÇAS")
    sys.exit(0 if ok else 1)


if __name__ == "__main__":
    main()
