#!/usr/bin/env python3
"""O contêiner de itens por instância (H2), em jogo (`H_NAME=horizon` ou `horizon2`, mundo de teste), contra o que o original faz
(`dev/referencia/rastros/dropper_*.jsonl`, `config_lados_*`, `slots_*`).

    h2_container_instancia.py medir    um Crusher com a configuração de lados posta por `data merge`, e um dropper, um dispenser e um hopper em cada face:
                                       entra só pela face que a configuração deixa (e o item que a máquina recusa fica no dropper, sem cair no chão), o hopper de
                                       baixo só leva o que o slot deixa tirar, e a entrada não sai pela face INPUT_OUTPUT. Sai 0 só se tudo é o do original.
    h2_container_instancia.py cena     a cena montada, um print (`PASTA` no desktop)
    h2_container_instancia.py video    o roteiro do vídeo com som: um dropper e um hopper alimentando o Crusher por faces diferentes, conforme a configuração
                                       (`NOME=<nome do gravar-jogo start>`, `SEGUNDOS`, `ARQUIVO`; veja `video()`)

Clima limpo e meio-dia com os ciclos parados; as gamerules voltam ao fim. A ordem dos lados da configuração é a de `RelativeSide` (frente, esquerda, direita, costas,
alto, base) e o DataType é o `ordinal` (0 NONE, 1 INPUT, 4 OUTPUT, 7 INPUT_OUTPUT, 8 ENERGY).
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

MACHINE = "mekanism:crusher"
COBBLE = "minecraft:cobblestone"
GRAVEL = "minecraft:gravel"
COAL = "minecraft:coal"
# O Crusher de frente para o norte: a esquerda é o leste, a direita o oeste e as costas o sul. Plataforma de `h2_nota_instrumento` (y 149), a máquina dois blocos acima para caber um hopper embaixo.
O = (66, 152, 84)
CAMERA = (66.5, 153.4, 88.4, 180, 14)
UP, DOWN, EAST, WEST, SOUTH, NORTH = (0, 1, 0), (0, -1, 0), (1, 0, 0), (-1, 0, 0), (0, 0, 1), (0, 0, -1)


def at(base, offset, times=1):
    return tuple(b + o * times for b, o in zip(base, offset))


def sides(front=0, left=0, right=0, back=0, top=0, bottom=0):
    return [front, left, right, back, top, bottom]


def config(item, energy=None, eject=False):
    return c.config_nbt(item=item, energy=energy if energy is not None else sides(right=1), eject=eject)


def merge(pos, body):
    c.command(f"data merge block {pos[0]} {pos[1]} {pos[2]} {{{body}}}")


def clear_scene():
    c.command(f"fill {O[0] - 3} {O[1] - 2} {O[2] - 3} {O[0] + 3} {O[1] + 3} {O[2] + 3} minecraft:air")
    time.sleep(0.4)


def machine(item_sides, *, items="", energy=None, eject=False):
    """O Crusher posto por `setblock` (tudo em NONE) com a configuração dada e os itens dados (na ordem dos slots)."""
    clear_scene()
    c.setblock(O, f"{MACHINE}[facing=north]")
    time.sleep(0.6)
    merge(O, f"Items:[{items}]," + config(item_sides, energy, eject))


def powered():
    """O Heat Generator a oeste (a direita do Crusher), com carvão: energia pela face da direita (INPUT na configuração de energia)."""
    generator = at(O, WEST)
    c.setblock(generator, "mekanismgenerators:heat_generator[facing=east]")
    time.sleep(0.8)
    merge(generator, f'Items:[{{Slot:0b,id:"{COAL}",count:64}}]')


def slot_count(pos, slot, item):
    held = c.read_items(pos).get(slot)
    return held[1] if held and held[0] == item.replace("minecraft:", "") else 0


def fire(dropper):
    """Liga o dropper ou o dispenser por um bloco de redstone atrás dele e espera os 4 ticks do agendamento."""
    behind = at(dropper, EAST)
    c.setblock(behind, "minecraft:redstone_block")
    c.step(8)
    c.setblock(behind, "minecraft:air")
    c.step(2)


def dropper_case(name, item_sides, expected_in, *, block="dropper", item=COBBLE):
    machine(item_sides)
    dropper = at(O, EAST)
    c.setblock(dropper, f'minecraft:{block}[facing=west]{{Items:[{{Slot:0b,id:"{item}",count:3}}]}}')
    time.sleep(0.5)
    fire(dropper)
    inside = slot_count(O, 0, item)
    left = sum(count for (found, count) in c.read_items(dropper).values() if found == item.replace("minecraft:", ""))
    got = (inside, left)
    return c.report(f"{block} {name}", expected_in, got)


def measure():
    ok = True
    c.command("tick freeze")
    n.sky()
    n.clear()
    # O dropper à esquerda (leste). Com a face em INPUT o item entra e o dropper dá um; sem DataType, com OUTPUT ou com ENERGY a face não deixa entrar e o item fica no dropper
    # (o original não solta nada no chão: `dropper_*.jsonl`, `dropped` 0). INPUT_OUTPUT deixa entrar.
    ok &= dropper_case("esquerda em INPUT: entra", sides(left=1), (1, 2))
    ok &= dropper_case("esquerda em INPUT_OUTPUT: entra", sides(left=7), (1, 2))
    ok &= dropper_case("esquerda sem DataType: fica no dropper", sides(), (0, 3))
    ok &= dropper_case("esquerda em OUTPUT: fica no dropper", sides(left=4), (0, 3))
    ok &= dropper_case("esquerda em ENERGY: fica no dropper", sides(left=8), (0, 3))
    ok &= dropper_case("item que a máquina não usa: fica no dropper", sides(left=1), (0, 3), item="minecraft:stick")
    # O dispenser nunca insere (`DefaultDispenseItemBehavior`): solta o item.
    ok &= dropper_case("esquerda em INPUT: solta e não insere", sides(left=1), (0, 2), block="dispenser")
    # Hopper em cima: só pela face em INPUT. Hopper embaixo: só leva o que o slot deixa tirar de quem está de fora (a saída; a entrada não).
    for name, top, want in (("em cima em INPUT: entra", 1, True), ("em cima sem DataType: não entra", 0, False), ("em cima em OUTPUT: não entra", 4, False)):
        machine(sides(top=top))
        c.setblock(at(O, UP), f'minecraft:hopper[facing=down]{{Items:[{{Slot:0b,id:"{COBBLE}",count:10}}]}}')
        time.sleep(0.5)
        c.step(40)
        ok &= c.report(f"hopper {name}", want, slot_count(O, 0, COBBLE) > 0)
    for name, bottom, want in (("embaixo em OUTPUT: leva o cascalho", 4, True), ("embaixo em INPUT: não leva", 1, False), ("embaixo sem DataType: não leva", 0, False)):
        machine(sides(bottom=bottom), items=f'{{Slot:1b,id:"{GRAVEL}",count:10}}')
        under = at(O, DOWN)
        c.setblock(under, "minecraft:hopper[facing=south]")
        time.sleep(0.5)
        c.step(40)
        ok &= c.report(f"hopper {name}", want, c.total(under, GRAVEL) > 0)
    # INPUT_OUTPUT embaixo: o hopper leva só a saída (cascalho), nunca a entrada (pedra), como o ejetor (`InputInventorySlot` não deixa o de fora tirar).
    machine(sides(bottom=7), items=f'{{Slot:0b,id:"{COBBLE}",count:10}},{{Slot:1b,id:"{GRAVEL}",count:10}}')
    under = at(O, DOWN)
    c.setblock(under, "minecraft:hopper[facing=south]")
    time.sleep(0.5)
    c.step(80)
    ok &= c.report("hopper embaixo em INPUT_OUTPUT: leva o cascalho", (True, False), (c.total(under, GRAVEL) > 0, c.total(under, COBBLE) > 0))
    # O ejetor de outra máquina entra pelo mesmo contêiner (`native_insert_item`): o Crusher da direita (oeste) recebe o cascalho que o primeiro ejeta, pela face que tem INPUT
    # (a esquerda dele, a leste); sem DataType nessa face, nada entra. Visto em jogo, sem rastro do Java.
    for name, left, want in (("com a face em INPUT: recebe", 1, True), ("sem DataType: não recebe", 0, False)):
        machine(sides(right=4), items=f'{{Slot:1b,id:"{GRAVEL}",count:10}}', eject=True)
        neighbour = at(O, WEST)
        c.setblock(neighbour, f"{MACHINE}[facing=north]")
        time.sleep(0.6)
        merge(neighbour, "Items:[]," + config(sides(left=left)))
        c.step(60)
        ok &= c.report(f"ejetor de outra máquina {name}", want, slot_count(neighbour, 0, GRAVEL) > 0)
    print("TUDO IGUAL" if ok else "HÁ DIFERENÇAS")
    return 0 if ok else 1


def scene(desk=None):
    n.sky()
    n.clear()
    machine(sides(left=1, top=0, bottom=4), items="")
    powered()
    c.setblock(at(O, DOWN), "minecraft:hopper[facing=south]")
    c.setblock(at(at(O, DOWN), SOUTH), "minecraft:chest")
    c.setblock(at(O, UP), f'minecraft:hopper[facing=down]{{Items:[{{Slot:0b,id:"{COBBLE}",count:64}}]}}')
    c.setblock(at(O, EAST), f'minecraft:dropper[facing=west]{{Items:[{{Slot:0b,id:"{COBBLE}",count:16}}]}}')
    n.h.command("tp @s %s %s %s %s %s" % CAMERA)
    time.sleep(1.0)
    if desk:
        c.ctl("screenshot", desk)


def video():
    """Um dropper à esquerda e um hopper em cima alimentando o Crusher, conforme a configuração de lados, com o som do jogo no vídeo.

    Primeiro as duas faces sem DataType: o dropper dispara e o item fica nele, o hopper não entrega (a máquina com a tela aberta mostra a entrada vazia). Depois a esquerda vai a INPUT e o
    dropper alimenta a máquina; depois o alto vai a INPUT e o hopper alimenta também; a base em OUTPUT leva o cascalho para o hopper de baixo. O volume mestre só sobe depois de o
    stream do container estar no sink nulo, e uma thread confere a cada meio segundo (`seguro`). Variáveis: `NOME`, `SEGUNDOS` (padrão 50), `ARQUIVO` (padrão container.mp4)."""
    nome, segundos, arquivo = os.environ["NOME"], os.environ.get("SEGUNDOS", "50"), os.environ.get("ARQUIVO", "container.mp4")
    n.sky()
    c.command("tick unfreeze")
    n.clear()
    scene()
    # Sem DataType em nenhuma face de item: nada entra por nenhuma.
    merge(O, config(sides(bottom=4)))
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

    def vigia():
        while not parar.is_set():
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
        dropper = at(O, EAST)

        def pulse():
            behind = at(dropper, EAST)
            c.setblock(behind, "minecraft:redstone_block")
            time.sleep(0.5)
            c.setblock(behind, "minecraft:air")
            time.sleep(1.0)

        # 1. Nenhuma face deixa entrar: o dropper dispara duas vezes e o item fica nele.
        for _ in range(2):
            pulse()
        c.ctl("interact", *map(str, O))
        time.sleep(3.0)
        # 2. A esquerda em INPUT: o dropper alimenta a máquina (a entrada sobe e a máquina liga).
        merge(O, config(sides(left=1, bottom=4)))
        for _ in range(4):
            pulse()
        time.sleep(2.0)
        # 3. O alto em INPUT: o hopper de cima também alimenta; a base em OUTPUT já leva o cascalho para o hopper de baixo.
        merge(O, config(sides(left=1, top=1, bottom=4)))
        time.sleep(12.0)
        c.ctl("close")
        time.sleep(1.0)
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
        elif mode == "cena":
            scene(sys.argv[2] if len(sys.argv) > 2 else None)
        elif mode == "video":
            video()
    finally:
        n.restore()
    sys.exit(code)
