#!/usr/bin/env python3
"""O dispenser com um balde contra um bloco com `fluid_logged` (decisão 99), em jogo (`H_NAME=horizon2`, mundo de teste), contra o que o original faz
(`dev/referencia/rastros/dispenser_bucket.jsonl`, de `DispenserBucketTraceTest.java`).

    h2_dispenser_gerador.py medir   (`SO=heat_generator,stone` só nesses blocos) as 90 fases do rastro: o bloco (Heat Generator, cubo de energia, Enrichment Chamber, cabo, cano e pedra) vazio, com lava e com água dentro, um dispenser
                                    ao lado com o balde de lava, o de água, o vazio, o de peixe e o de neve em pó: o que sobra no dispenser, o `fluid_logged` do bloco e o item solto, 12 ticks
                                    depois, são os do rastro. Sai 0 só se tudo é igual.
    h2_dispenser_gerador.py cena    a cena montada, prints (`SHOTS` no desktop)
    h2_dispenser_gerador.py video   o roteiro do vídeo com som (`NOME=<gravar-jogo start>`, `SEGUNDOS`, `ARQUIVO`)

Clima limpo e meio-dia com os ciclos parados; as gamerules voltam ao fim.
"""
import json
import os
import re
import subprocess
import sys
import threading
import time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import h2_config_lados as c
import h2_nota_instrumento as n
import h2_cubo_gerador as g

O = g.O
RASTRO = os.path.join(g.RASTROS, "dispenser_bucket.jsonl")
BLOCKS = {
    "heat_generator": "mekanismgenerators:heat_generator[facing=west]",
    "basic_energy_cube": "mekanism:basic_energy_cube[facing=down]",
    "enrichment_chamber": "mekanism:enrichment_chamber[facing=west]",
    "basic_universal_cable": "mekanism:basic_universal_cable",
    "basic_mechanical_pipe": "mekanism:basic_mechanical_pipe",
    "stone": "minecraft:stone",
}
FACE = "east"


def with_fluid(block, pre):
    """O bloco com o `fluid_logged` dado, juntando-o às propriedades que o bloco já leva."""
    if pre is None:
        return block
    if block.endswith("]"):
        return block[:-1] + f",fluid_logged={pre}]"
    return block + f"[fluid_logged={pre}]"


def state_logged(pos):
    state = json.loads(c.ctl("block", str(pos[0]), str(pos[1]), str(pos[2])))["state"]
    found = re.search(r"fluid_logged=(\w+)", state)
    return found.group(1) if found else "-"


def held(dispenser):
    items = c.read_items(dispenser)
    return [[item, count] for _, (item, count) in sorted(items.items())]


BUCKET_ITEMS = ("lava_bucket", "water_bucket", "bucket", "cod_bucket", "powder_snow_bucket")


def dropped_near(pos, items=BUCKET_ITEMS):
    """Os itens soltos junto de `pos` por tipo, somando os que o `kill` conta (e matando-os, para a próxima fase não ver os de antes)."""
    counts = {}
    for item in items:
        namespaced = item if ":" in item else f"minecraft:{item}"
        selector = f"@e[type=minecraft:item,nbt={{Item:{{id:\"{namespaced}\"}}}},x={pos[0]},y={pos[1]},z={pos[2]},distance=..4]"
        chat = " ".join(c.command(f"kill {selector}"))
        found = re.search(r"Killed (\d+) entit", chat)
        if found:
            counts[item] = int(found.group(1))
        elif chat.startswith("Killed "):
            counts[item] = 1
    return [[item, count] for item, count in counts.items()]


def run_case(row, block):
    c.command("fill %d %d %d %d %d %d minecraft:air" % (O[0] - 3, O[1] - 2, O[2] - 3, O[0] + 4, O[1] + 3, O[2] + 3))
    time.sleep(0.2)
    pre = row["pre"] if row["logged"] != "-" else None
    c.setblock(O, with_fluid(block, pre))
    time.sleep(0.3)
    dispenser = c.at(O, FACE)
    count = 2 if row["item"] == "bucket" else 1
    c.setblock(dispenser, f'minecraft:dispenser[facing=west]{{Items:[{{Slot:0b,id:"minecraft:{row["item"]}",count:{count}}}]}}')
    time.sleep(0.3)
    behind = c.at(dispenser, FACE)
    c.setblock(behind, "minecraft:redstone_block")
    c.step(12)
    c.setblock(behind, "minecraft:air")
    c.step(1)
    return state_logged(O), held(dispenser), dropped_near(O)


def measure():
    ok = True
    c.command("tick freeze")
    n.sky()
    n.clear()
    rows = [json.loads(line) for line in open(RASTRO)]
    only = os.environ.get("SO")
    if only:
        rows = [row for row in rows if row["target"] in only.split(",")]
    for row in rows:
        got_logged, got_held, got_dropped = run_case(row, BLOCKS[row["target"]])
        name = f"{row['target']} {row['pre']} {row['item']}"
        ok &= c.report(f"{name} fluid_logged", row["logged"], got_logged)
        ok &= c.report(f"{name} dispenser", row["dispenser"], got_held)
        # A lava queima o item solto que cai nela (o cabo e o cano ficam abertos), e é o `fluido-mob-item` (decisão 88) que a faz queimar: só o que não está na lava se compara.
        if row["pre"] != "lava":
            ok &= c.report(f"{name} solto", sorted(row["dropped"]), sorted(got_dropped))
    print("TUDO IGUAL" if ok else "HÁ DIFERENÇAS")
    return 0 if ok else 1


ITEM_TRACE = os.path.join(g.RASTROS, "dispenser_item_trace.jsonl")


def items():
    """O item solto de um dispenser diante do Heat Generator (`dispenser_item_trace.jsonl`): o Energy Tablet, o graveto e o carvão, com a energia em 1.000, vazio e com lava dentro,
    por quatro faces. No original nenhum dos 240 sumiu e o dispenser ficou com 2 (3 menos o solto), o tablet como os outros: o sumiço do rastro `dropper_heat_generator.jsonl` foi a lava que
    corre para fora do gerador nas fases seguintes. Aqui cada combinação roda duas vezes."""
    ok = True
    c.command("tick freeze")
    n.sky()
    n.clear()
    seen = set()
    for line in open(ITEM_TRACE):
        row = json.loads(line)
        face, pre, rest = row["case"].split("_", 2)
        item = rest.split("@")[0]
        key = (face, pre, item)
        if key in seen:
            continue
        seen.add(key)
        item_id = {"energy_tablet": "mekanism:energy_tablet"}.get(item, f"minecraft:{item}")
        for repeat in range(2):
            c.command("fill %d %d %d %d %d %d minecraft:air" % (O[0] - 3, O[1] - 2, O[2] - 3, O[0] + 4, O[1] + 3, O[2] + 3))
            # Os ticks agendados da lava do caso anterior (30 ticks) correm sobre o ar, sem lava para espalhar: cada caso começa limpo, como os 240 do rastro (um por vez, 15 ticks).
            c.step(35)
            c.setblock(O, with_fluid("mekanismgenerators:heat_generator[facing=west]", pre if pre != "empty" else None))
            time.sleep(0.6)
            g.merge(O, "energy:1000")
            dispenser = c.at(O, face)
            opposite = g.OPPOSITE[face]
            c.setblock(dispenser, f'minecraft:dispenser[facing={opposite}]{{Items:[{{Slot:0b,id:"{item_id}",count:3}}]}}')
            time.sleep(0.3)
            behind = c.at(dispenser, face)
            c.setblock(behind, "minecraft:redstone_block")
            c.step(12)
            c.setblock(behind, "minecraft:air")
            c.step(1)
            left = sum(count for (found, count) in c.read_items(dispenser).values())
            solto = dropped_near(O, (item_id,))
            ok &= c.report(f"{face} {pre} {item} #{repeat}", ([[item_id.split(":")[1] if item_id.startswith("minecraft:") else item_id.split(":")[1], 2]], [[item_id, 1]]), ([[item, left]], solto))
    print("TUDO IGUAL" if ok else "HÁ DIFERENÇAS")
    return 0 if ok else 1


SHOTS = "/home/luka/.cache/rust-mekanism/dispenser-gerador/shots"


def shot(name):
    c.ctl("screenshot", f"/tmp/{name}.png")
    subprocess.run(["ssh", "desktop", f"mkdir -p {SHOTS} && docker exec {os.environ.get('H_NAME', 'horizon2')} cat /tmp/{name}.png > {SHOTS}/{name}.png"], check=True)


SCENE = (66, 150, 84)


def build():
    """O Heat Generator no chão da plataforma (y 150), virado para oeste com três dispensers: o do leste com o balde de lava, o do sul com o de água e o do norte com o balde vazio."""
    c.command("fill %d %d %d %d %d %d minecraft:air" % (SCENE[0] - 6, SCENE[1], SCENE[2] - 6, SCENE[0] + 7, SCENE[1] + 4, SCENE[2] + 6))
    time.sleep(0.4)
    c.setblock(SCENE, "mekanismgenerators:heat_generator[facing=west]")
    c.setblock(c.at(SCENE, "east"), 'minecraft:dispenser[facing=west]{Items:[{Slot:0b,id:"minecraft:lava_bucket",count:1}]}')
    c.setblock(c.at(SCENE, "south"), 'minecraft:dispenser[facing=north]{Items:[{Slot:0b,id:"minecraft:water_bucket",count:1}]}')
    c.setblock(c.at(SCENE, "north"), 'minecraft:dispenser[facing=south]{Items:[{Slot:0b,id:"minecraft:bucket",count:1}]}')
    time.sleep(0.5)


def pulse(face, pause=1.6):
    dispenser = c.at(SCENE, face)
    behind = c.at(dispenser, face)
    c.setblock(behind, "minecraft:redstone_block")
    time.sleep(0.8)
    c.setblock(behind, "minecraft:air")
    time.sleep(pause)


def refill(face, item):
    """Outro balde no dispenser que já gastou o dele (o de água foi recusado e caiu no chão)."""
    dispenser = c.at(SCENE, face)
    g.merge(dispenser, f'Items:[{{Slot:0b,id:"minecraft:{item}",count:1}}]')
    time.sleep(0.3)


def scene():
    c.command("tick unfreeze")
    n.sky()
    c.ctl("options-set", "chatVisibility", "2")
    build()
    g.camera(62.5, 150.0, 80.5, -37, 12)
    shot("dispenser_antes")
    pulse("east")
    time.sleep(1.5)
    shot("dispenser_lava")
    pulse("south")
    time.sleep(1.0)
    shot("dispenser_agua_recusada")
    pulse("north")
    time.sleep(1.5)
    shot("dispenser_balde_vazio_tira")
    refill("south", "water_bucket")
    pulse("south")
    time.sleep(1.5)
    shot("dispenser_agua")
    c.ctl("options-set", "chatVisibility", "0")


def video():
    """Os três dispensers contra o Heat Generator: a lava entra no gerador (o balde fica vazio no dispenser), o balde de água é recusado (o gerador já tem lava) e cai no chão, o dispenser
    com o balde vazio tira a lava de volta, e o de água agora enche o gerador de água. Com o som do jogo no vídeo. Variáveis: `NOME`, `SEGUNDOS` (padrão 40), `ARQUIVO` (padrão dispenser.mp4)."""
    nome, segundos, arquivo = os.environ["NOME"], os.environ.get("SEGUNDOS", "40"), os.environ.get("ARQUIVO", "dispenser.mp4")
    n.sky()
    c.command("tick unfreeze")
    build()
    g.camera(62.5, 150.0, 80.5, -37, 12)
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
        pulse("east", 3.5)   # a lava entra no gerador
        pulse("south", 3.5)  # o balde de água é recusado e cai
        pulse("north", 3.5)  # o balde vazio tira a lava
        refill("south", "water_bucket")
        pulse("south", 3.5)  # agora a água entra
        time.sleep(4.0)
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
        elif mode == "itens":
            code = items()
        elif mode == "cena":
            scene()
        elif mode == "video":
            video()
    finally:
        n.restore()
    sys.exit(code)
