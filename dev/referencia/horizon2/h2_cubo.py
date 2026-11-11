#!/usr/bin/env python3
"""Os cubos de energia em jogo (`H_NAME=horizon`, mundo plano): a lógica do rastro `energy_cube_*` e o desenho (portas, LEDs, núcleo).

    H_NAME=horizon h2_cubo.py checar        confere pelo NBT: sem config nada passa, o gerador enche o cubo, o cubo alimenta a máquina, os slots de carga e descarga, o criativo
    H_NAME=horizon h2_cubo.py fotos DIR     prints do modelo (portas, LEDs, núcleo) de cada tier e de cada configuração
    H_NAME=horizon h2_cubo.py cena          a cena do vídeo: um Heat Generator enche o cubo e o cubo alimenta uma Enrichment Chamber (rode o `gravar-jogo rec` em paralelo)

O servidor anda por `tick step` nas conferências. A ordem dos lados no NBT é a de `RelativeSide` (frente, esquerda, direita, costas, alto, base) e o `DataType` é o ordinal
(0 NONE, 1 INPUT, 4 OUTPUT, 7 INPUT_OUTPUT).
"""
import json, os, re, subprocess, sys, time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import h2_identity as h

BASE = (80, -60, 80)
GENERATOR = "mekanismgenerators:heat_generator"
CHAMBER = "mekanism:enrichment_chamber"
TIERS = ["basic", "advanced", "elite", "ultimate", "creative"]
failures = []


def cmd(text):
    return h.command(text)[0]


def at(dx=0, dy=0, dz=0):
    return (BASE[0] + dx, BASE[1] + dy, BASE[2] + dz)


def text(p):
    return f"{p[0]} {p[1]} {p[2]}"


def setblock(p, block):
    cmd(f"setblock {text(p)} {block}")


def step(ticks=1):
    cmd(f"tick step {ticks}")
    time.sleep(0.35 + 0.05 * ticks)


def number(raw):
    found = re.search(r"data: (-?\d+)", raw) or re.search(r"(-?\d+)[bBsSlLfFdD]?$", raw.strip())
    return int(found.group(1)) if found else None


def energy(p):
    return number(cmd(f"data get block {text(p)} energy")) or 0


def long_energy(p):
    return number(cmd(f"data get block {text(p)} energy_amount"))


def expect(label, got, want):
    ok = got == want
    print(("OK   " if ok else "DIFF ") + label + ("" if ok else f"  esperado {want}  obtido {got}"))
    if not ok:
        failures.append(label)


def config(energy_sides=None, item_sides=None, eject=True):
    parts = []
    if energy_sides is not None:
        parts.append("config0:[I;" + ",".join(map(str, energy_sides)) + "]")
        parts.append(f"eject0:{1 if eject else 0}b")
    if item_sides is not None:
        parts.append("config3:[I;" + ",".join(map(str, item_sides)) + "]")
    return "{component_config:{" + ",".join(parts) + "}}"


DEFAULT_ENERGY = [4, 1, 1, 1, 1, 1]
DEFAULT_ITEM = [4, 1, 1, 1, 1, 1]


def prep():
    """Clima limpo e meio-dia, com os ciclos parados (pedido do usuário para toda captura; nesta versão as gamerules são `advance_time` e `advance_weather`)."""
    for text_ in ["weather clear", "time set noon", "gamerule advance_time false", "gamerule advance_weather false"]:
        cmd(text_)


def clear_area():
    cmd(f"fill {text(at(-8, 0, -10))} {text(at(24, 6, 12))} minecraft:air")
    time.sleep(1.0)


def put_cube(p, tier, facing="north", sides=None, items=None, eject=True):
    setblock(p, f"mekanism:{tier}_energy_cube[facing={facing}]")
    time.sleep(0.5)
    step(2)
    cmd(f"data merge block {text(p)} " + config(sides or DEFAULT_ENERGY, items or DEFAULT_ITEM, eject))
    step(2)


def checar():
    cmd("tick freeze")
    clear_area()
    cube, gen, chamber = at(0, 0, 1), at(0, 0, 2), at(0, 0, 0)
    # 1. posto por setblock, sem configuração: nada passa.
    setblock(cube, "mekanism:basic_energy_cube[facing=north]")
    setblock(gen, f"{GENERATOR}[facing=north]")
    setblock(chamber, f"{CHAMBER}[facing=north]")
    time.sleep(0.5)
    step(2)
    cmd(f"data merge block {text(gen)} {{Items:[{{Slot:0b,id:\"minecraft:coal\",count:64}}]}}")
    step(40)
    expect("setblock sem config: o cubo não recebeu", energy(cube), 0)
    # 2. com a configuração do item: o gerador enche o cubo e o cubo alimenta a máquina.
    cmd(f"data merge block {text(cube)} " + config(DEFAULT_ENERGY, DEFAULT_ITEM, True))
    cmd(f"data merge block {text(chamber)} {{Items:[{{Slot:0b,id:\"minecraft:clay\",count:16}}]}}")
    # A máquina também só aceita energia nas faces em INPUT (setblock = NONE).
    cmd(f"data merge block {text(chamber)} " + config([1, 1, 1, 1, 1, 1], [1, 1, 1, 1, 1, 1], False))
    step(60)
    e_cube, e_chamber = energy(cube), energy(chamber)
    print("cubo", e_cube, "máquina", e_chamber, "gerador", energy(gen))
    expect("o gerador encheu o cubo (energia > 0 ou a máquina já recebeu)", (e_cube or 0) > 0 or (e_chamber or 0) > 0, True)
    # Com o cubo cheio de verdade, a máquina anda: 1.000 de energia por tick de trabalho, vindos do cubo pela frente.
    cmd(f"data merge block {text(cube)} {{energy:1000000}}")
    step(250)
    items = h.read_items(chamber)
    print("máquina:", items, "cubo", energy(cube))
    expect("a máquina processou com a energia do cubo", any(v[1] == "clay_ball" for v in items), True)
    # 3. slot de carga: o tablet recebe do cubo.
    cmd(f"data merge block {text(gen)} {{Items:[]}}")
    cmd(f"data merge block {text(cube)} {{energy:3000000}}")
    cmd(f"data merge block {text(cube)} {{Items:[{{Slot:1b,id:\"mekanism:energy_tablet\",count:1}}]}}")
    step(10)
    raw = cmd(f"data get block {text(cube)} Items")
    print("slots:", raw[:300])
    expect("o tablet na carga ganhou energia", "mekanism:energy" in raw, True)
    expect("o cubo gastou 5000 por tick no tablet", (energy(cube) or 0) < 3000000, True)
    # 4. slot de descarga: redstone vira energia.
    cmd(f"data merge block {text(cube)} {{energy:0,Items:[{{Slot:0b,id:\"minecraft:redstone\",count:5}}]}}")
    step(10)
    e = energy(cube)
    print("depois de 10 ticks com 5 redstone:", e)
    expect("a redstone virou energia (5 x 10000 menos o que saiu)", e is not None and e > 0, True)
    # 5. criativo.
    creative = at(4, 0, 1)
    put_cube(creative, "creative", "north", [4, 4, 4, 4, 4, 4], [4, 1, 1, 1, 1, 1], True)
    cmd(f"data merge block {text(creative)} {{energy_amount:9223372036854775807L}}")
    far = at(4, 0, 0)
    setblock(far, "mekanism:basic_energy_cube[facing=north]")
    step(2)
    cmd(f"data merge block {text(far)} " + config([1, 1, 1, 1, 1, 1], DEFAULT_ITEM, False))
    step(30)
    expect("o criativo entregou ao vizinho", (energy(far) or 0) > 100000, True)
    expect("o criativo não gastou", long_energy(creative), 9223372036854775807)
    print("TUDO IGUAL" if not failures else "HÁ DIFERENÇAS: " + ", ".join(failures))
    return not failures


def camera(p, dx, dy, dz, yaw, pitch):
    cmd(f"tp @s {p[0] + dx + 0.5} {p[1] + dy} {p[2] + dz + 0.5} {yaw} {pitch}")
    time.sleep(0.6)


def shot(path):
    time.sleep(0.8)
    h.ctl("screenshot", path)
    print("print", path)


def fotos(out):
    cmd("tick unfreeze")
    prep()
    clear_area()
    os.makedirs(out, exist_ok=True)
    for index, tier in enumerate(TIERS):
        p = at(index * 3, 0, 0)
        put_cube(p, tier, "north", DEFAULT_ENERGY if tier != "creative" else [4, 4, 4, 4, 4, 4])
        cmd(f"data merge block {text(p)} {{energy:{[2000000, 8000000, 32000000, 128000000, 2000000][index]}}}")
    time.sleep(2)
    cmd("effect give @s minecraft:night_vision 99999 1 true")
    camera(at(6, 0, 0), 0, 2, -6, 0, 15)
    shot(f"{out}/00_tiers_frente.png")
    camera(at(6, 0, 0), 0, 2, 6, 180, 15)
    shot(f"{out}/01_tiers_costas.png")


def cena():
    cmd("tick unfreeze")
    prep()
    clear_area()
    cube, gen, chamber = at(0, 0, 1), at(0, 0, 2), at(0, 0, 0)
    setblock(chamber, f"{CHAMBER}[facing=south]")
    put_cube(cube, "basic", "north", [4, 0, 0, 1, 0, 0], DEFAULT_ITEM, True)
    setblock(gen, f"{GENERATOR}[facing=north]")
    time.sleep(0.5)
    cmd(f"data merge block {text(chamber)} " + config([1, 1, 1, 1, 1, 1], [1, 1, 1, 1, 1, 1], False))
    cmd(f"data merge block {text(chamber)} {{Items:[{{Slot:0b,id:\"minecraft:clay\",count:64}}]}}")
    cmd(f"data merge block {text(gen)} {{Items:[{{Slot:0b,id:\"minecraft:coal\",count:64}}]}}")
    cmd("effect give @s minecraft:night_vision 99999 1 true")
    cmd(f"data merge block {text(cube)} {{energy:2500000}}")
    camera(cube, 3, 1, 3, 135, 15)
    time.sleep(45)


STAGES = {"checar": checar, "fotos": lambda: fotos(sys.argv[2]), "cena": cena}



def quebra():
    """O drop por quebra com o mouse de verdade (olhar o bloco e segurar o ataque, em sobrevivência): a energia e a configuração vão no item que cai e voltam ao repor."""
    cmd("tick unfreeze")
    prep()
    clear_area()
    p = at(0, 0, 0)
    cmd("gamemode survival")
    cmd("clear @s")
    cmd("item replace entity @s hotbar.0 with minecraft:diamond_pickaxe")
    cmd("item replace entity @s hotbar.1 with mekanism:basic_energy_cube[mekanism:energy=1234567L]")
    cmd(f"tp @s {p[0] + 0.5} {p[1]} {p[2] - 2.5} 0 0")
    h.ctl("select", "2")
    out = json.loads(h.ctl("place", *map(str, p), "--item", "mekanism:basic_energy_cube"))
    time.sleep(1.5)
    expect("posto pelo jogador", "error" not in out, True)
    expect("a energia do item entrou no tile", energy(p), 1234567)
    cmd(f"data merge block {text(p)} " + config([4, 4, 0, 1, 1, 1], DEFAULT_ITEM, True))
    time.sleep(0.5)
    h.ctl("select", "1")
    h.ctl("look-at-block", *map(str, p))
    time.sleep(0.5)
    h.ctl("mouse", "left", "down")
    for _ in range(60):
        time.sleep(0.25)
        if "air" in cmd(f"data get block {text(p)}") or "not a block entity" in cmd(f"data get block {text(p)}"):
            break
    h.ctl("mouse", "left", "up")
    time.sleep(2.0)
    raw = cmd("data get entity @e[type=minecraft:item,limit=1,sort=nearest,nbt={Item:{id:\"mekanism:basic_energy_cube\"}}] Item")
    print("item que caiu:", raw[-420:])
    expect("o item solto leva a energia", "1234567" in raw, True)
    expect("o item solto leva a configuração de lados", "mekanism:side_config" in raw, True)
    cmd("gamemode creative")
    for text_ in ["gamerule advance_time true", "gamerule advance_weather true"]:
        print(text_, "->", h.command(text_))
    print("TUDO IGUAL" if not failures else "HÁ DIFERENÇAS: " + ", ".join(failures))
    return not failures


STAGES["quebra"] = quebra
if __name__ == "__main__":
    sys.exit(0 if STAGES[sys.argv[1]]() in (True, None) else 1)
