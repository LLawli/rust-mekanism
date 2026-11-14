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
    """O jogador põe os cabos um a um (o som do bloco posto) e o modelo de cada um muda com os vizinhos: o centro sozinho, o braço `NORMAL`, o trecho reto (o `hideContiguousJoin`: o centro
    da linha mostra a textura do braço) e a junção em T. A câmera é a do jogador, parada ao norte da linha."""
    clear_noon()
    cmd("fill %d %d %d %d %d %d minecraft:air" % (X - 4, Y, Z - 4, X + 8, Y + 4, Z + 4))
    cmd("gamemode creative")
    cmd("tick unfreeze")
    h.ctl("give", "mekanism:basic_universal_cable", "16")
    cmd("tp @s %.2f %.2f %.2f 0 28" % (X + 2.5, Y, Z - 3.0))
    time.sleep(3.0)
    gravar = f"{GRAVADOR} rec cabo-modelo {out} {seconds}" + (f" {container}" if container else "")
    proc = subprocess.Popen(["ssh", "desktop", gravar])
    time.sleep(2.5)
    for x in (X, X + 1, X + 2, X + 3):
        place("mekanism:basic_universal_cable", x)
        time.sleep(2.5)
    # Uma junção em T: o cabo do meio ganha o braço do sul.
    h.ctl("select", "mekanism:basic_universal_cable")
    h.ctl("look-at", "%.2f" % (X + 1.5), "%.2f" % Y, "%.2f" % (Z + 1.5))
    time.sleep(1.0)
    h.ctl("mouse", "right", "click")
    time.sleep(3.0)
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
