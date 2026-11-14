#!/usr/bin/env python3
"""O vídeo das pilhas andando por uma linha de transportadores (frente `transportador-visual`, Fase 4, decisões 215 a 217). Os prints são de `h2_blocos.py cena` com
`ROTEIRO=transportador_visual.json` e `transportador_hotbar.json`.

    H_NAME=horizon ./h2_transportador_visual.py video SAIDA.mp4 SEGUNDOS CONTAINER   # o Configurator liga o puxão do baú ao primeiro transportador azul e os itens andam
    H_NAME=horizon ./h2_transportador_visual.py devolver                              # devolve as gamerules e limpa a cena

A captura é a do `gravar-jogo rec` (PipeWire do gamescope, com o som do jogo num sink nulo próprio; veja `procedures/gravar-video-real-do-jogo.md`). Mundo plano (chão em y -61), clima
limpo e meio-dia."""
import os, subprocess, sys, time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
os.environ.setdefault("H_NAME", "horizon")
import h2_identity as h
import h2_ajustes_jogo as aj
import h2_blocos as blocos

GRAVADOR = "~/.cache/rust-mekanism/gravar-jogo"
Y, Z, X0 = -60, 82, 60
GAMERULES = ("advance_time", "advance_weather")
ITENS = ["minecraft:cobblestone", "minecraft:redstone", "minecraft:diamond", "minecraft:stick", "minecraft:oak_log", "minecraft:coal", "minecraft:apple", "minecraft:iron_ingot"]
cmd = aj.cmd
# O caminho do `gerar_cena_transportador_visual.py`: três em x, a curva para o sul, mais dois, a curva para o leste e outros dois.
CAMINHO = [(X0 + 1, Z), (X0 + 2, Z), (X0 + 3, Z), (X0 + 3, Z + 1), (X0 + 3, Z + 2), (X0 + 4, Z + 2), (X0 + 5, Z + 2)]
AZUL = 1


def clear_noon():
    cmd("weather clear")
    cmd("time set noon")
    for rule in GAMERULES:
        cmd(f"gamerule {rule} false")


def linha():
    cmd("fill %d %d %d %d %d %d minecraft:air" % (X0 - 2, Y, Z - 2, X0 + 10, Y + 4, Z + 6))
    cmd(f"setblock {X0} {Y} {Z} minecraft:chest")
    for x, z in CAMINHO:
        cmd(f"setblock {x} {Y} {z} mekanism:basic_logistical_transporter")
    cmd(f"setblock {X0 + 6} {Y} {Z + 2} minecraft:chest")
    for i, item in enumerate(ITENS):
        cmd(f"item replace block {X0} {Y} {Z} container.{i} with {item} 16")
    time.sleep(1.5)
    for x, z in CAMINHO:
        blocos.cor(f"#cor {x} {Y} {z} {AZUL}")


def video(out, seconds, container):
    """O Configurator agachado liga o puxão (`PULL`) do primeiro transportador no baú, e os itens andam pela linha (as curvas, as duas cores de pilha pela do transportador). O jogador fica
    a 8 blocos acima, de cabeça para baixo (`/tp` com o pitch 90), onde o clique cai no braço."""
    clear_noon()
    cmd("gamemode creative")
    cmd("tick unfreeze")
    h.ctl("give", "mekanism:configurator", "1")
    linha()
    cmd("tp @s %.2f %.2f %.2f 0 90" % (X0 + 3.5, Y + 9.0, Z + 1.5))
    time.sleep(3.0)
    gravar = f"{GRAVADOR} rec transportador-visual {out} {seconds}" + (f" {container}" if container else "")
    proc = subprocess.Popen(["ssh", "desktop", gravar])
    time.sleep(3.0)
    blocos.conexao(f"#conexao {CAMINHO[0][0]} {Y} {CAMINHO[0][1]} west=pull")
    cmd("tp @s %.2f %.2f %.2f 0 90" % (X0 + 3.5, Y + 9.0, Z + 1.5))
    proc.wait()


def devolver():
    for rule in GAMERULES:
        cmd(f"gamerule {rule} true")
    cmd("fill %d %d %d %d %d %d minecraft:air" % (X0 - 2, Y, Z - 2, X0 + 10, Y + 4, Z + 6))


if __name__ == "__main__":
    if sys.argv[1] == "video":
        video(sys.argv[2], int(sys.argv[3]) if len(sys.argv) > 3 else 24, sys.argv[4] if len(sys.argv) > 4 else "")
    else:
        devolver()
