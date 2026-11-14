#!/usr/bin/env python3
"""O vídeo das partes do modelo do transmissor (frente `cabo-modelo`, Fase 4, decisões 206 e 207). Os prints são de `h2_blocos.py cena` com `ROTEIRO=cabo_modelo.json`.

    H_NAME=horizon2 h2_cabo_modelo.py video SAIDA.mp4 SEGUNDOS CONTAINER   # o Configurator cicla o braço leste de um cabo (`PUSH`, `PULL`, `NONE`, `NORMAL`), e o centro vira trecho reto
    H_NAME=horizon2 h2_cabo_modelo.py devolver                              # devolve as gamerules e limpa a cena

A captura é a do `gravar-jogo rec` (PipeWire do gamescope, com o som do jogo num sink nulo próprio; veja `procedures/gravar-video-real-do-jogo.md`). O jogo é o mundo plano da cena
(chão em y -61), clima limpo e meio-dia."""
import os, subprocess, sys, time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
os.environ.setdefault("H_NAME", "horizon2")
import h2_identity as h
import h2_ajustes_jogo as aj

GRAVADOR = "~/.cache/rust-mekanism/gravar-jogo"
Y, Z, X = -60, 82, 62
GAMERULES = ("advance_time", "advance_weather")
cmd = aj.cmd


def clear_noon():
    cmd("weather clear")
    cmd("time set noon")
    for rule in GAMERULES:
        cmd(f"gamerule {rule} false")


def cena():
    cmd("fill %d %d %d %d %d %d minecraft:air" % (X - 4, Y, Z - 4, X + 8, Y + 4, Z + 4))
    # Como os tiros da cena: o cabo do meio entre outro cabo e uma máquina, e uma segunda máquina ao sul.
    cmd(f"setblock {X} {Y} {Z} mekanism:basic_universal_cable")
    cmd(f"setblock {X + 1} {Y} {Z} mekanism:basic_universal_cable")
    cmd(f"setblock {X + 2} {Y} {Z} mekanism:enrichment_chamber")
    cmd(f"setblock {X + 1} {Y} {Z + 1} mekanism:enrichment_chamber")


def place(block_item, x):
    """Põe um bloco como jogador (o som do bloco posto): olha o topo do chão sob `x` e o botão direito."""
    h.ctl("select", block_item)
    h.ctl("look-at", "%.2f" % (x + 0.5), "%.2f" % Y, "%.2f" % (Z + 0.5))
    time.sleep(1.0)
    h.ctl("mouse", "right", "click")
    time.sleep(1.0)
    print("posto em", x, h.ctl("block", str(x), str(Y), str(Z)).split('"state"')[0][:80])


def video(out, seconds, container):
    """O Configurator troca o tipo do braço leste de um cabo que dá numa máquina (`PUSH`, `PULL`, `NONE`, `NORMAL`) e o modelo muda. O jogador fica parado a 3 blocos em cima do braço, de
    cabeça para baixo (`/tp` com o pitch 90, sem `look-at`, que faz a câmera derivar), agachado, e o clique cai no braço (a máquina é só um bloco ao lado, fora da mira). O som é o do
    bloco posto: o jogador põe o cabo do meio antes."""
    clear_noon()
    cmd("fill %d %d %d %d %d %d minecraft:air" % (X - 4, Y, Z - 4, X + 8, Y + 4, Z + 4))
    cmd("gamemode creative")
    cmd("tick unfreeze")
    cmd(f"setblock {X} {Y} {Z} mekanism:basic_universal_cable")
    cmd(f"setblock {X + 2} {Y} {Z} mekanism:enrichment_chamber")
    h.ctl("give", "mekanism:basic_universal_cable", "4")
    h.ctl("give", "mekanism:configurator", "1")
    arm = (X + 1 + aj.ARM["east"][0], Y + 3.0, Z + 0.5)
    cmd("tp @s %.2f %.2f %.2f 0 90" % arm)
    time.sleep(3.0)
    gravar = f"{GRAVADOR} rec cabo-modelo {out} {seconds}" + (f" {container}" if container else "")
    proc = subprocess.Popen(["ssh", "desktop", gravar])
    time.sleep(2.5)
    h.ctl("select", "mekanism:basic_universal_cable")
    h.ctl("look-at", "%.2f" % (X + 1.5), "%.2f" % Y, "%.2f" % (Z + 0.5))
    time.sleep(1.0)
    h.ctl("mouse", "right", "click")
    time.sleep(2.5)
    cmd("tp @s %.2f %.2f %.2f 0 90" % arm)
    h.ctl("select", "mekanism:configurator")
    h.ctl("key", "shift", "down")
    time.sleep(1.5)
    for _ in range(4):
        h.ctl("mouse", "right", "click")
        time.sleep(2.5)
    h.ctl("key", "shift", "up")
    proc.wait()


def devolver():
    for rule in GAMERULES:
        cmd(f"gamerule {rule} true")
    cmd("fill %d %d %d %d %d %d minecraft:air" % (X - 4, Y, Z - 4, X + 8, Y + 4, Z + 4))


if __name__ == "__main__":
    if sys.argv[1] == "video":
        video(sys.argv[2], int(sys.argv[3]) if len(sys.argv) > 3 else 24, sys.argv[4] if len(sys.argv) > 4 else "")
    else:
        devolver()
