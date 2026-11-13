#!/usr/bin/env python3
"""A dimensão que o jogador deixa (frente `descarga-nivel`, decisões 193 a 195): o vídeo do jogador indo ao Nether e voltando, com o leitor de rede no cabo antes e depois.

    SOCK=<nome do gravar-jogo> H_NAME=horizon2 h2_descarga_nivel.py video

O roteiro com os logs e as sondas é o `h2_reset_mundo.py roteiro` (os passos `a4_no_nether` e `a4_de_volta`, e as saídas do mundo); este só grava a cena com som. O jogo já tem de estar aberto
no título, com `SOCK` do `gravar-jogo start` (`JOGO_PW_SOCK` no container) e o sink conferido por `audio_nulo.sh`.
"""
import os, subprocess, sys, time

TOOLS = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, TOOLS)
os.environ.setdefault("H_NAME", "horizon2")
import h2_reset_mundo as base
from h2_reset_mundo import cmd, ctl, wait, state, CABLE

YV = -60


def video():
    import h2_ajustes_jogo as a
    import h2_network_reader as nr
    sock = os.environ["SOCK"]
    nome = os.environ.get("NOME", "descarga_nivel")
    local = os.path.expanduser(os.environ.get("LOCAL", "~/.cache/rust-mekanism/wa/descarga-nivel"))
    a.SHOTS = "/home/luka/.cache/rust-mekanism/descarga-nivel/prints"
    a.LOCAL = local
    if state().get("world") != "ResetV":
        ctl("world-delete", "ResetV", "--confirm")
        import h2_mundo_plano as mp
        sys.argv = ["h2_mundo_plano.py", "ResetV", "20261115"]
        mp.main()
    cmd("gamemode creative @s")
    cmd("clear @s")
    cmd("weather clear")
    ctl("time", "6000")
    cmd("gamerule advance_time false")
    cmd("gamerule advance_weather false")
    ctl("options-set", "chatVisibility", "0")
    ctl("options-set", "guiScale", "3")
    cmd("fill 0 -61 0 4 -61 0 minecraft:grass_block")
    cmd("fill 0 -60 0 4 -60 0 minecraft:air")
    wait(40)
    for x in range(5):
        cmd(f"setblock {x} {YV} 0 {CABLE}")
    wait(40)
    cmd(f"data merge block 0 {YV} 0 {{energy:12345}}")
    wait(40)
    ctl("give", nr.READER, "1")
    wait(10)
    ctl("select", nr.READER)
    stand = (2.5, -60.0, -2.4)
    cmd("tp @s %.2f %.2f %.2f" % stand)
    wait(240)
    a.CROP = "1000x300+0+680"
    ctl("options-set", "chatVisibility", "0")
    for categoria in ("master", "block", "ui", "player"):
        ctl("options-set", f"soundCategory_{categoria}", "1.0")
    proc = subprocess.Popen(["ssh", "desktop", f"~/.cache/rust-mekanism/gravar-jogo rec {sock} {nome}.mp4 55 {os.environ['H_NAME']}"], stdout=subprocess.DEVNULL)
    time.sleep(2.0)
    try:
        nr.clicar((2, YV, 0), (0.5, 0.5, 0.5), sneak=False, stand=stand)
        antes = a.shot("nivel_antes")
        wait(40)
        cmd("execute in minecraft:the_nether run tp @s 0.5 100 0.5")
        wait(160)
        cmd("execute in minecraft:overworld run tp @s %.2f %.2f %.2f" % stand)
        wait(100)
        ctl("select", nr.READER)
        wait(20)
        nr.clicar((2, YV, 0), (0.5, 0.5, 0.5), sneak=False, stand=stand)
        depois = a.shot("nivel_depois")
        wait(40)
    finally:
        proc.wait()
        ctl("options-set", "soundCategory_master", "0.0")
    a.stack(antes, depois, os.path.join(local, "nivel_antes_depois.png"))


if __name__ == "__main__":
    if len(sys.argv) > 1 and sys.argv[1] == "video":
        video()
    else:
        raise SystemExit("uso: h2_descarga_nivel.py video")
