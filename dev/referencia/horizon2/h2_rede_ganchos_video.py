#!/usr/bin/env python3
"""O vídeo da frente `rede-ganchos`: o cabo de energia segue levando energia do Heat Generator à Enrichment Chamber com os ganchos dos transmissores ligados (a sonda `MBT` no log).

    H_NAME=horizon NOME=<gravar-jogo start> [SEGUNDOS=30] [ARQUIVO=rede_ganchos.mp4] h2_rede_ganchos_video.py video
    H_NAME=horizon h2_rede_ganchos_video.py cena     um print da cena (`h2_cubo_gerador.SHOTS`, no desktop)

A cena: o gerador a leste com carvão, o cabo no meio, a máquina a oeste com ferro no slot de entrada e a energia aceita só pela face do cabo. A máquina acende e a barra anda. Clima limpo
e meio-dia com os ciclos parados; o volume mestre só sobe depois de o stream estar no sink de teste (`seguro`).
"""
import os
import subprocess
import sys
import threading
import time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import h2_cubo_gerador as g
import h2_config_lados as c
import h2_nota_instrumento as n

MACHINE = "mekanism:enrichment_chamber"
CABLE = "mekanism:basic_universal_cable"
CAMERA = (g.O[0] + 1.5, g.O[1] + 1.2, g.O[2] + 4.0, 180, 4)


def scene():
    n.sky()
    c.command("tick unfreeze")
    n.clear()
    g.clear_scene()
    machine = g.O
    cable = g.at(machine, "east")
    generator = g.at(machine, "east", 2)
    c.setblock(machine, f"{MACHINE}[facing=south]")
    c.setblock(cable, CABLE)
    c.setblock(generator, "mekanismgenerators:heat_generator[facing=west]")
    time.sleep(0.8)
    # INPUT de energia em todas as faces (o cabo entrega pela do leste).
    g.merge(machine, 'Items:[{Slot:0b,id:"minecraft:iron_ore",count:32}],' + c.config_nbt(item=g.sides(front=1, left=1, right=1, back=1, top=1, bottom=1), energy=g.sides(front=1, left=1, right=1, back=1, top=1, bottom=1)))
    g.merge(generator, 'Items:[{Slot:0b,id:"minecraft:coal",count:64}]')
    c.command("tp @s %s %s %s %s %s" % CAMERA)


def video():
    nome, segundos, arquivo = os.environ["NOME"], os.environ.get("SEGUNDOS", "30"), os.environ.get("ARQUIVO", "rede_ganchos.mp4")
    scene()
    n.mute()
    c.ctl("options-set", "chatVisibility", "2")
    for categoria in n.SOUND_OFF:
        c.ctl("options-set", f"soundCategory_{categoria}", "0")
    for categoria in ("block", "record"):
        c.ctl("options-set", f"soundCategory_{categoria}", "1.0")
    for _ in range(60):
        if "movidos 0" not in n.desktop(f"ligar {n.CONTAINER}").stdout and n.desktop(f"verificar {n.CONTAINER}").stdout.startswith("ok"):
            break
        time.sleep(1)
    else:
        print("o stream do jogo não foi para o sink nulo: nada gravado")
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
        # O gerador queima 64 de carvão em 200 a 400 ticks: repõe a cada 3 s. Aos 10 s o cabo sai e aos 14 s volta: a máquina para e volta.
        cable = g.at(g.O, "east")
        generator = g.at(g.O, "east", 2)
        start = time.time()
        broken = restored = False
        while time.time() - start < float(segundos) - 8:
            g.merge(generator, 'Items:[{Slot:0b,id:"minecraft:coal",count:64}]')
            if not broken and time.time() - start > 10:
                c.setblock(cable, "minecraft:air")
                broken = True
            if broken and not restored and time.time() - start > 14:
                c.setblock(cable, CABLE)
                restored = True
            time.sleep(3.0)
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
    try:
        if sys.argv[1] == "video":
            video()
        else:
            scene()
            time.sleep(6)
            g.shot("rede_ganchos")
    finally:
        n.restore()
