#!/usr/bin/env python3
"""O vídeo com som dos comandos de vários blocos sobre uma fila de cabos e máquinas (frente `eventos-meio`, decisões 187 a 189), gravado de verdade pela captura PipeWire do gamescope num
PipeWire privado (`~/.cache/rust-mekanism/gravar-jogo`, ai-memory `procedures/gravar-video-real-do-jogo`), com o som do jogo juntado (`gravar-jogo rec ... CONTAINER`).

  H_NAME=horizon h2_eventos_meio_video.py rodar SOCKNOME     o roteiro; grava o mp4 em `~/.cache/rust-mekanism/gravador/saida/eventos_meio.mp4`
  H_NAME=horizon h2_eventos_meio_video.py prints PASTA       só as ações, com um print a cada passo em PASTA (dentro do container)

O roteiro, no chão de teste de `h2_rede_ganchos.py` (clima limpo e meio-dia, ciclos parados): `/fill` de cinco cabos, `/fill` de cinco máquinas ao lado, o meio da fila trocado por ar e
reposto, `/clone` da fila, uma pedra ao lado, as máquinas tiradas e, no fim, uma explosão no meio da fila (o som). O jogo tem de ter nascido com `JOGO_PW_SOCK` e com `gamescope -r 30`
(`FPS=30 abrir_horizon.sh`), e o volume só sobe depois de `audio_nulo.sh verificar` dar `ok`."""
import os
import subprocess
import sys
import time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import h2_rede_ganchos as g
import h2_identity as h

NAME = os.environ.get("H_NAME", "horizon")
CABLE, CHAMBER = g.CABLE, g.CHAMBER


def pos(rel):
    return g.text(g.abs_pos(rel))


def fill(a, b, block):
    g.chat(f"fill {pos(a)} {pos(b)} {block}")
    h.ctl("wait", "26")


def arena():
    g.chat("weather clear")
    g.chat("time set noon")
    g.chat("gamerule advance_time false")
    g.chat("gamerule advance_weather false")
    h.ctl("options-set", "chatVisibility", "2")
    h.ctl("options-set", "guiScale", "3")
    g.chat("gamerule send_command_feedback false")
    g.prepare()


def camera():
    x, y, z = g.BASE
    g.chat(f"tp @s {x + 3.5} {y + 5} {z - 4} 0 42")
    h.ctl("wait", "10")
    h.ctl("look-at", f"{x + 3.5:.3f}", f"{y - 0.5:.3f}", f"{z + 2.0:.3f}")
    h.ctl("wait", "10")


def roteiro(print_dir=None):
    n = [0]

    def foto(nome):
        if print_dir:
            n[0] += 1
            h.ctl("screenshot", f"{print_dir}/eventos_{n[0]:02d}_{nome}.png")

    camera()
    foto("antes")
    fill((1, 1, 2), (5, 1, 2), CABLE)
    foto("fila_de_cabos")
    fill((1, 1, 1), (5, 1, 1), CHAMBER)
    foto("fila_de_maquinas")
    fill((2, 1, 2), (3, 1, 2), "minecraft:air")
    foto("meio_trocado_por_ar")
    fill((2, 1, 2), (3, 1, 2), CABLE)
    g.chat(f"clone {pos((1, 1, 2))} {pos((5, 1, 2))} {pos((1, 1, 3))}")
    h.ctl("wait", "26")
    foto("fila_clonada")
    g.chat(f"setblock {pos((0, 1, 2))} minecraft:stone")
    h.ctl("wait", "20")
    fill((1, 1, 1), (5, 1, 1), "minecraft:air")
    x, y, z = g.abs_pos((3, 1, 2))
    g.chat(f"summon minecraft:tnt {x}.5 {y}.5 {z}.5 {{fuse:20}}")
    h.ctl("wait", "50")
    foto("depois_da_explosao")
    h.ctl("wait", "20")


def gravado(sock):
    arena()
    camera()
    h.ctl("options-set", "soundCategory_master", "1.0")
    h.ctl("options-set", "soundCategory_block", "1.0")
    h.ctl("options-set", "soundCategory_player", "1.0")
    proc = subprocess.Popen(["ssh", "desktop", f"~/.cache/rust-mekanism/gravar-jogo rec {sock} eventos_meio.mp4 30 {NAME}"], stdout=subprocess.DEVNULL)
    time.sleep(2.0)
    try:
        roteiro()
    finally:
        proc.wait()
        h.ctl("options-set", "soundCategory_master", "0.0")


if __name__ == "__main__":
    modo = sys.argv[1] if len(sys.argv) > 1 else "rodar"
    if modo == "prints":
        arena()
        roteiro(sys.argv[2])
    else:
        gravado(sys.argv[2])
