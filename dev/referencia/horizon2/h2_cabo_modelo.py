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
    cmd(f"setblock {X} {Y} {Z} mekanism:basic_universal_cable")
    cmd(f"setblock {X + 1} {Y} {Z} mekanism:basic_universal_cable")
    cmd(f"setblock {X + 2} {Y} {Z} mekanism:enrichment_chamber")


def video(out, seconds, container):
    clear_noon()
    cena()
    cmd("gamemode creative")
    h.ctl("give", "mekanism:configurator", "1")
    middle = (X + 1, Y, Z)
    eye = (middle[0] + 0.5, middle[1] + 1.0, middle[2] - 2.6)
    cmd("tp @s %.2f %.2f %.2f 0 18" % eye)
    time.sleep(3.0)
    gravar = f"{GRAVADOR} rec cabo-modelo {out} {seconds}" + (f" {container}" if container else "")
    proc = subprocess.Popen(["ssh", "desktop", gravar])
    time.sleep(2.5)
    # O braço leste do cabo do meio, que dá na máquina: `PUSH`, `PULL`, `NONE` e de volta a `NORMAL`.
    for _ in range(4):
        aj.click_at(middle, aj.ARM["east"], eye)
        time.sleep(2.0)
    # A máquina sai e um cabo entra no lugar: o centro vira trecho reto (o `hideContiguousJoin`), e a ponta do outro lado volta a ser um braço só.
    cmd(f"setblock {X + 2} {Y} {Z} mekanism:basic_universal_cable")
    time.sleep(3.0)
    cmd(f"setblock {X + 2} {Y} {Z} minecraft:air")
    time.sleep(2.5)
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
