#!/usr/bin/env python3
"""A 12.ª rodada da frente `tanques` em jogo, no port (`H_NAME=horizon`, um cliente, com o jogo aberto):

    h2_tanques12.py chunk     o Fluid Tank com lava posto num chunk que o cliente ainda não tem (longe, por `forceload`), e a luz do mundo do cliente quando o jogador chega
    h2_tanques12.py video     a cena do vídeo com som: o balde na fonte (o som do balde), a tela do tanque e o clique na aba do modo (o som da `ui`)

O que mede contra o original (o clique inteiro do cliente nos dois jogos, com o convidado de LAN) está em `cenas/dois_tanques12.txt` (`h2_dois_roteiro.py` no port e
`oficial_dois.sh` com `testes/CenaDois.java` no original).
"""
import json, os, sys, time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import h2_identity as h
import h2_tanques9 as t9

# Longe do jogador (fora da distância de visão): o chunk do tanque só chega ao cliente quando ele vai lá.
LONGE = (2000, 140, 2000)


def chunk():
    x, y, z = LONGE
    h.command("forceload add %d %d" % (x, z))
    time.sleep(1)
    h.command("setblock %d %d %d minecraft:glass" % (x, y - 1, z))
    h.command("setblock %d %d %d mekanism:basic_fluid_tank{%s}" % (x, y, z, t9.contents("port", "fluid", "minecraft:lava", 32000)))
    time.sleep(1)
    print("antes de ir (o cliente não tem o chunk):", h.ctl("light", str(x - 2), str(y), str(z), str(x + 2), str(y), str(z)).strip()[:300])
    h.command("forceload remove %d %d" % (x, z))
    # Em criativo: o jogador chega no ar e, em sobrevivência, cairia.
    h.command("gamemode creative")
    h.command("tp @s %d %d %d" % (x + 4, y, z))
    h.ctl("--timeout", "120", "wait-ready")
    for espera in (0, 1, 3):
        time.sleep(espera)
        print("depois de %d s:" % espera, h.ctl("light", str(x - 8), str(y), str(z), str(x + 8), str(y), str(z)).strip()[:400])
    h.command("setblock %d %d %d minecraft:air" % (x, y, z))
    h.command("setblock %d %d %d minecraft:air" % (x, y - 1, z))


def video():
    """Uns 40 s: o jogador diante de uma fonte de água num chão de vidro, o item no modo balde tira a água (o som do balde), agachado a devolve, e depois a tela de um
    tanque posto, com o clique na aba do modo (o som da `ui`)."""
    bx, by, bz = 58, 142, 82
    h.command("time set noon")
    h.command("weather clear")
    h.command("gamerule advance_time false")
    h.command("gamerule advance_weather false")
    h.command("gamemode survival")
    h.command("fill %d %d %d %d %d %d minecraft:glass" % (bx - 2, by - 1, bz - 2, bx + 2, by - 1, bz + 4))
    h.command("setblock %d %d %d minecraft:water" % (bx, by - 1, bz + 2))
    h.command("setblock %d %d %d mekanism:basic_fluid_tank" % (bx + 2, by, bz + 2))
    h.command("tp @s %.1f %d %.1f 0 40" % (bx + 0.5, by, bz + 0.5))
    h.command("clear @s")
    h.command("item replace entity @s hotbar.0 with mekanism:basic_fluid_tank[mekanism:bucket_mode=true]")
    h.ctl("select", "1")
    time.sleep(2)
    # A posse, depois o balde na fonte e de volta, agachado.
    for passo in ("posse", "tira", "poe", "tira"):
        if passo == "poe":
            h.ctl("key", "shift", "down")
            time.sleep(0.4)
        h.ctl("look-at", "%.1f" % (bx + 0.5), "%.2f" % (by - 0.1), "%.1f" % (bz + 2.5))
        time.sleep(0.5)
        h.ctl("use-item")
        time.sleep(2.5)
        if passo == "poe":
            h.ctl("key", "shift", "up")
    # A tela do tanque e o clique na aba do modo.
    h.ctl("interact", str(bx + 2), str(by), str(bz + 2))
    time.sleep(3)
    print(h.ctl("screen")[:200])
    time.sleep(2)
    for _ in range(3):
        print(h.ctl("click-at", "%d" % (-(-1920 // 3) // 2 - 88 + 188), "%d" % (-(-1080 // 3) // 2 - 83 + 151)).strip()[:120])
        time.sleep(2.5)
    h.ctl("close")
    time.sleep(1)
    h.command("fill %d %d %d %d %d %d minecraft:air" % (bx - 2, by - 1, bz - 2, bx + 2, by, bz + 4))
    h.command("gamerule advance_time true")
    h.command("gamerule advance_weather true")


if __name__ == "__main__":
    for parte in sys.argv[1:]:
        {"chunk": chunk, "video": video}[parte]()
