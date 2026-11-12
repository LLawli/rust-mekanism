#!/usr/bin/env python3
"""O contêiner de itens por instância no cubo de energia e no Heat Generator (H2, decisão 94), em jogo (`H_NAME=horizon2`, mundo de teste), contra o que o original faz
(`dev/referencia/rastros/dropper_energy_cube_basic.jsonl` e `dropper_heat_generator.jsonl`, de `DropperTraceTest.java`).

    h2_cubo_gerador.py medir    um dropper em cada face, com cada item do rastro e com a energia vazia e com 1.000, apontado para o cubo (virado para baixo, como o do rastro) e
                                para o Heat Generator (virado para oeste): quantos itens o dropper ainda tem, 10 ticks depois, é o do rastro. Depois o hopper de baixo leva do
                                cubo e do gerador o que o slot deixa tirar. Sai 0 só se tudo é igual.
    h2_cubo_gerador.py ritmo    um hopper de baixo e um de cima em cada bloco contra o mesmo hopper num baú: o mesmo número de itens movidos em 160 ticks (o ritmo não dobra,
                                nem cai)
    h2_cubo_gerador.py cena     a cena montada, um print (`PASTA` no desktop)
    h2_cubo_gerador.py video    o roteiro do vídeo com som: um dropper alimentando o Heat Generator com carvão (`NOME=<gravar-jogo start>`, `SEGUNDOS`, `ARQUIVO`)

Clima limpo e meio-dia com os ciclos parados; as gamerules voltam ao fim. A ordem dos lados da configuração é a de `RelativeSide` (frente, esquerda, direita, costas, alto, base)
e o DataType é o `ordinal` (0 NONE, 1 INPUT, 4 OUTPUT).
"""
import json
import os
import subprocess
import sys
import threading
import time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import h2_config_lados as c
import h2_nota_instrumento as n

CUBE = "mekanism:basic_energy_cube"
GENERATOR = "mekanismgenerators:heat_generator"
RASTROS = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "rastros")
# Plataforma de `h2_nota_instrumento` (y 149), o bloco dois acima para caber um hopper embaixo.
O = (66, 152, 84)
CAMERA = (66.5, 154.0, 90.0, 180, 20)
DIRECTIONS = {"down": (0, -1, 0), "up": (0, 1, 0), "north": (0, 0, -1), "south": (0, 0, 1), "west": (-1, 0, 0), "east": (1, 0, 0)}
OPPOSITE = {"down": "up", "up": "down", "north": "south", "south": "north", "west": "east", "east": "west"}


def at(base, face, times=1):
    return tuple(b + d * times for b, d in zip(base, DIRECTIONS[face]))


def merge(pos, body):
    c.command(f"data merge block {pos[0]} {pos[1]} {pos[2]} {{{body}}}")


def clear_scene():
    c.command(f"fill {O[0] - 3} {O[1] - 2} {O[2] - 3} {O[0] + 3} {O[1] + 3} {O[2] + 3} minecraft:air")
    time.sleep(0.4)


def sides(front=0, left=0, right=0, back=0, top=0, bottom=0):
    return [front, left, right, back, top, bottom]


def place_cube(energy, items=""):
    """O cubo de energia virado para baixo (a frente do rastro) com a configuração do item (tudo entrada e a frente saída) e a energia dada."""
    clear_scene()
    c.setblock(O, f"{CUBE}[facing=down]")
    time.sleep(0.6)
    merge(O, f"Items:[{items}],energy:{energy}," + c.config_nbt(item=sides(front=4, left=1, right=1, back=1, top=1, bottom=1), energy=sides(front=4, left=1, right=1, back=1, top=1, bottom=1)))
    time.sleep(0.2)


def place_generator(energy, items=""):
    clear_scene()
    c.setblock(O, f"{GENERATOR}[facing=west]")
    time.sleep(0.8)
    merge(O, f"Items:[{items}],energy:{energy}")
    time.sleep(0.2)


def item_name(path):
    return {"energy_tablet": "mekanism:energy_tablet", "bio_fuel": "mekanism:bio_fuel"}.get(path, "minecraft:" + path)


def dropper_left(dropper, path):
    return sum(count for (found, count) in c.read_items(dropper).values() if found == item_name(path).replace("minecraft:", ""))


def fire(dropper, face):
    """Liga o dropper por um bloco de redstone atrás dele e espera os ticks do agendamento e os 10 do rastro."""
    behind = at(dropper, face)
    c.setblock(behind, "minecraft:redstone_block")
    c.step(12)
    c.setblock(behind, "minecraft:air")
    c.step(1)


def read_trace(name):
    rows = []
    with open(os.path.join(RASTROS, f"dropper_{name}.jsonl")) as handle:
        for line in handle:
            rows.append(json.loads(line))
    return rows


def dropper_cases(name, place):
    """Cada fase `dropper_<face>_<item>@<energia>` do rastro, repetida em jogo."""
    ok = True
    rows = read_trace(name)
    for row in rows[1:]:
        phase = row["phase"]
        if not phase.startswith("dropper_"):
            continue
        rest, energy = phase[len("dropper_"):].split("@")
        face, path = rest.split("_", 1)
        place(int(energy))
        count = 1 if path == "lava_bucket" else 3
        dropper = at(O, face)
        c.setblock(dropper, f'minecraft:dropper[facing={OPPOSITE[face]}]{{Items:[{{Slot:0b,id:"{item_name(path)}",count:{count}}}]}}')
        time.sleep(0.3)
        fire(dropper, face)
        want = row["dropper"][0][1] if row["dropper"] else 0
        ok &= c.report(f"{name} {phase}", want, dropper_left(dropper, path))
    return ok


def hopper_below(name, place, wanted):
    """O hopper embaixo (a frente do cubo e a base do gerador, as duas saem por baixo no rastro): o que ele leva do slot 0."""
    ok = True
    for path, want in wanted:
        place(0, f'{{Slot:0b,id:"{item_name(path)}",count:1}}')
        under = at(O, "down")
        c.setblock(under, "minecraft:hopper[facing=south]")
        time.sleep(0.4)
        c.step(24)
        ok &= c.report(f"{name} hopper embaixo leva {path}", want, c.total(under, item_name(path)) > 0)
    return ok


def measure():
    ok = True
    c.command("tick freeze")
    n.sky()
    n.clear()
    ok &= dropper_cases("energy_cube_basic", place_cube)
    ok &= dropper_cases("heat_generator", place_generator)
    # O que o hopper de baixo leva do slot 0 (extract_down_<item>@0 do rastro): o cubo (a frente, o slot de descarga) tira tudo menos o que converte em energia; o gerador (o slot de
    # combustível) tira o que não é combustível. O carvão e o balde de lava queimam ou enchem o tanque no tick: não dá para ver em jogo, e o teste de regra os cobre.
    ok &= hopper_below("cubo", place_cube, [("cobblestone", True), ("energy_tablet", True), ("bucket", True)])
    ok &= hopper_below("gerador", place_generator, [("cobblestone", True), ("redstone", True), ("energy_tablet", True), ("bucket", True)])
    print("TUDO IGUAL" if ok else "HÁ DIFERENÇAS")
    return 0 if ok else 1


def pace():
    """O ritmo do hopper: o vanilla move um item a cada 8 ticks. Um hopper embaixo do cubo (slot 0 com 64 pedras) e um embaixo do gerador (64 pedras no combustível) contra um
    hopper embaixo de um baú com 64 pedras; um hopper em cima do cubo e do gerador (com carvão, que o gerador queima) contra um hopper em cima de um baú."""
    c.command("tick freeze")
    n.sky()
    n.clear()
    ticks = 160
    results = {}

    def run(label, setup, count):
        setup()
        time.sleep(0.5)
        c.step(ticks)
        results[label] = count()

    def below_of(place, label):
        def setup():
            place(0, '{Slot:0b,id:"minecraft:cobblestone",count:64}')
            c.setblock(at(O, "down"), "minecraft:hopper[facing=south]")
        run(label, setup, lambda: c.total(at(O, "down"), "minecraft:cobblestone"))

    def control_below():
        def setup():
            clear_scene()
            c.setblock(O, "minecraft:chest")
            time.sleep(0.4)
            merge(O, 'Items:[{Slot:0b,id:"minecraft:cobblestone",count:64}]')
            c.setblock(at(O, "down"), "minecraft:hopper[facing=south]")
        run("baú embaixo", setup, lambda: c.total(at(O, "down"), "minecraft:cobblestone"))

    control_below()
    below_of(place_cube, "cubo embaixo")
    below_of(place_generator, "gerador embaixo")

    def above(place, label, item):
        def setup():
            place(0)
            c.setblock(at(O, "up"), f'minecraft:hopper[facing=down]{{Items:[{{Slot:0b,id:"{item}",count:64}}]}}')
        run(label, setup, lambda: 64 - c.total(at(O, "up"), item))

    def control_above(item):
        def setup():
            clear_scene()
            c.setblock(O, "minecraft:chest")
            c.setblock(at(O, "up"), f'minecraft:hopper[facing=down]{{Items:[{{Slot:0b,id:"{item}",count:64}}]}}')
        run("baú em cima", setup, lambda: 64 - c.total(at(O, "up"), item))

    # O cubo recebe pela base e pelo alto o que o slot de carga aceita; sem um item carregável o hopper de cima não move nada, então o controle é o tablet: um item só cabe.
    control_above("minecraft:coal")
    above(place_generator, "gerador em cima", "minecraft:coal")
    print(json.dumps(results, ensure_ascii=False))
    same = len({results["baú embaixo"], results["cubo embaixo"], results["gerador embaixo"]}) == 1 and results["baú em cima"] == results["gerador em cima"]
    print("RITMO IGUAL" if same else "RITMO DIFERENTE")
    return 0 if same else 1


SHOTS = "/home/luka/.cache/rust-mekanism/ccg/shots"


def shot(name):
    """Um print do jogo: o `horizon-ctl screenshot` grava dentro do container, e o arquivo vai para a pasta de prints do desktop."""
    c.ctl("screenshot", f"/tmp/{name}.png")
    subprocess.run(["ssh", "desktop", f"mkdir -p {SHOTS} && docker exec {os.environ.get('H_NAME', 'horizon2')} cat /tmp/{name}.png > {SHOTS}/{name}.png"], check=True)


def camera(x, y, z, yaw, pitch):
    n.h.command("tp @s %s %s %s %s %s" % (x, y, z, yaw, pitch))
    time.sleep(1.2)


def scene(desk=None):
    """O Heat Generator com um dropper de carvão em cima e outro nas costas, e o cubo ao lado, antes e depois de o dropper das costas disparar."""
    c.command("tick unfreeze")
    n.sky()
    n.clear()
    c.ctl("options-set", "chatVisibility", "2")
    place_generator(0)
    south, east = at(O, "south"), at(O, "east")
    c.setblock(south, 'minecraft:dropper[facing=north]{Items:[{Slot:0b,id:"minecraft:coal",count:8}]}')
    c.setblock(east, 'minecraft:dropper[facing=west]{Items:[{Slot:0b,id:"minecraft:coal",count:16}]}')
    camera(63.0, 150.0, 80.0, -38, -10)
    shot("gerador_antes")
    for dropper, face in ((south, "south"), (east, "east")):
        for _ in range(3):
            behind = at(dropper, face)
            c.setblock(behind, "minecraft:redstone_block")
            time.sleep(0.7)
            c.setblock(behind, "minecraft:air")
            time.sleep(1.0)
    time.sleep(2.0)
    shot("gerador_depois")
    cube_at = (O[0] - 4, O[1], O[2])
    c.setblock(cube_at, f"{CUBE}[facing=down]")
    time.sleep(0.6)
    merge(cube_at, "energy:0," + c.config_nbt(item=sides(front=4, left=1, right=1, back=1, top=1, bottom=1), energy=sides(front=4, left=1, right=1, back=1, top=1, bottom=1)))
    c.setblock(at(cube_at, "up"), 'minecraft:dropper[facing=down]{Items:[{Slot:0b,id:"mekanism:energy_tablet",count:3}]}')
    time.sleep(0.5)
    camera(59.0, 150.0, 80.0, -38, -14)
    shot("cubo_antes")
    behind = at(at(cube_at, "up"), "up")
    c.setblock(behind, "minecraft:redstone_block")
    time.sleep(1.5)
    c.setblock(behind, "minecraft:air")
    time.sleep(1.5)
    shot("cubo_depois")
    c.ctl("options-set", "chatVisibility", "0")


def video():
    """Um dropper alimentando o Heat Generator com carvão pela face certa, com o som do jogo no vídeo.

    O gerador olha para o oeste: a direita dele (o sul) é a do slot de energia, e as outras faces são a do combustível. Primeiro o dropper do sul dispara com carvão e o item fica nele (o slot de
    energia não aceita carvão); depois o dropper do leste (as costas) dispara e o carvão entra, o gerador acende e produz. O volume mestre só sobe depois de o stream do container estar no
    sink nulo, e uma thread confere a cada meio segundo (`seguro`). Variáveis: `NOME`, `SEGUNDOS` (padrão 40), `ARQUIVO` (padrão heat_dropper.mp4)."""
    nome, segundos, arquivo = os.environ["NOME"], os.environ.get("SEGUNDOS", "40"), os.environ.get("ARQUIVO", "heat_dropper.mp4")
    n.sky()
    c.command("tick unfreeze")
    n.clear()
    place_generator(0)
    time.sleep(0.5)
    south = at(O, "south")
    east = at(O, "east")
    c.setblock(south, 'minecraft:dropper[facing=north]{Items:[{Slot:0b,id:"minecraft:coal",count:8}]}')
    c.setblock(east, 'minecraft:dropper[facing=west]{Items:[{Slot:0b,id:"minecraft:coal",count:16}]}')
    n.h.command("tp @s %s %s %s %s %s" % (63.0, 150.0, 80.0, -38, -10))
    n.mute()
    c.ctl("options-set", "chatVisibility", "2")
    for categoria in n.SOUND_OFF:
        c.ctl("options-set", f"soundCategory_{categoria}", "0")
    for categoria in ("block", "record"):
        c.ctl("options-set", f"soundCategory_{categoria}", "1.0")
    ligado = False
    for _ in range(60):
        if "movidos 0" not in n.desktop(f"ligar {n.CONTAINER}").stdout and n.desktop(f"verificar {n.CONTAINER}").stdout.startswith("ok"):
            ligado = True
            break
        time.sleep(1)
    if not ligado:
        print("o stream do jogo não foi para o sink nulo: nada gravado, volume mestre em 0")
        sys.exit(1)
    parar = threading.Event()

    grav = f"som_grav_{nome}"
    ate = [None]

    def vigia():
        # O stream do jogo só existe enquanto algo toca: um som novo abre um stream novo, que segue o sink de teste (`som_nulo`) e não o da gravação. Até poucos segundos antes do fim
        # (o `rec` apaga o sink dele ao terminar), o vigia manda o stream do container para o sink da gravação.
        while not parar.is_set():
            if ate[0] is not None and time.time() < ate[0]:
                subprocess.run(["ssh", "desktop", f"AUDIO_SINK={grav} {n.AUDIO_NULO} ligar {n.CONTAINER}"], capture_output=True, text=True)
            if not n.desktop(f"seguro {n.CONTAINER}").stdout.startswith("ok"):
                print("stream fora dos sinks de teste: volume mestre em 0", flush=True)
                n.mute()
            parar.wait(0.5)

    thread = threading.Thread(target=vigia, daemon=True)
    thread.start()
    rec = None
    try:
        c.ctl("options-set", "soundCategory_master", "1.0")
        time.sleep(2.0)
        rec = subprocess.Popen(["ssh", "desktop", f"{n.GRAVAR} rec {nome} {arquivo} {segundos} {n.CONTAINER}"], stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
        time.sleep(3.0)
        ate[0] = time.time() + int(segundos) - 10

        def pulse(dropper, face):
            behind = at(dropper, face)
            c.setblock(behind, "minecraft:redstone_block")
            time.sleep(0.6)
            c.setblock(behind, "minecraft:air")
            time.sleep(1.2)

        # 1. O dropper do sul (a direita do gerador, o slot de energia) dispara duas vezes: o carvão fica nele.
        for _ in range(2):
            pulse(south, "south")
        time.sleep(1.5)
        # 2. O dropper do leste (as costas, o slot de combustível) dispara: o carvão entra e o gerador acende.
        for _ in range(6):
            pulse(east, "east")
        time.sleep(12.0)
        out, err = rec.communicate(timeout=int(segundos) + 90)
        print(out, err, flush=True)
    finally:
        n.mute()
        parar.set()
        thread.join(timeout=5)
        if rec is not None and rec.poll() is None:
            rec.wait(timeout=60)
        c.ctl("close")


if __name__ == "__main__":
    mode = sys.argv[1]
    code = 0
    try:
        if mode == "medir":
            code = measure()
        elif mode == "ritmo":
            code = pace()
        elif mode == "cena":
            scene(sys.argv[2] if len(sys.argv) > 2 else None)
        elif mode == "video":
            video()
    finally:
        n.restore()
    sys.exit(code)
