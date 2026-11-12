#!/usr/bin/env python3
"""O cubo de energia em jogo (`H_NAME=horizon`, mundo plano): o cubo posto de novo no mesmo lugar, o item e o núcleo sob `tick freeze`.

    h2_cubo_item.py repor PASTA     o cubo posto, apagado e posto de novo no mesmo lugar (o objeto vivo velho do `energy_cube.rs`): um print por rodada
    h2_cubo_item.py congelar PASTA  três prints seguidos com `tick freeze` (o núcleo não pode andar entre eles)
    h2_cubo_item.py video           o roteiro do vídeo (os cinco itens na mão e os cinco cubos postos, o núcleo girando); grave com `gravar-jogo rec` em paralelo

Clima limpo e meio-dia com os ciclos parados; as gamerules voltam ao fim.
"""
import os
import subprocess
import sys
import time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import h2_identity as h

SHOTS = "/home/luka/.local/share/jogo-isolado/horizon/data/shots/cubo-item"
BASE = (60, -60, 82)
TP = "62.5 -58.38 77.7 0 13.3"
CLEAR = "fill 52 -60 78 120 -52 90 minecraft:air"


def sky():
    h.ctl("close")
    for c in ("gamemode creative", "weather clear", "time set noon", "gamerule advance_time false", "gamerule advance_weather false", "gamerule spawn_mobs false", "difficulty peaceful", "kill @e[type=!minecraft:player]"):
        h.command(c)


def restore():
    h.command("tick unfreeze")
    h.command("gamerule advance_time true")
    h.command("gamerule advance_weather true")
    h.command("gamerule spawn_mobs true")


def shot(name):
    subprocess.run(["ssh", "desktop", f"mkdir -p {SHOTS}"], check=True)
    h.ctl("screenshot", f"{SHOTS}/{name}.png")
    print("print", name)


def place(fraction=0.5):
    x, y, z = BASE
    for i, tier in enumerate(("basic", "advanced")):
        h.command(f"setblock {x + i} {y} {z} mekanism:{tier}_energy_cube[facing=north]")
    time.sleep(1.5)
    for i, tier in enumerate(("basic", "advanced")):
        cap = {"basic": 4_000_000, "advanced": 16_000_000}[tier]
        h.command(f"data merge block {x + i} {y} {z} {{energy:{int(cap * fraction)},component_config:{{config0:[I;4,1,1,1,1,1]}}}}")


def repor(rodadas=3):
    sky()
    h.command("tick unfreeze")
    h.command("tp @s " + TP)
    for r in range(rodadas):
        # Como o roteiro `cenas/cubo.json`: o servidor parado entre dois tiros, o ar posto com ele parado, e o cubo posto logo que ele anda de novo.
        h.command("tick freeze")
        time.sleep(1.0)
        h.command(CLEAR)
        h.command("tick unfreeze")
        place()
        time.sleep(2.0)
        shot(f"repor_{r}")


ITENS = (
    ("basic", 4_000_000),
    ("advanced", 16_000_000),
    ("elite", 64_000_000),
    ("ultimate", 256_000_000),
    ("creative", 9223372036854775807),
)


def video():
    """Os cinco itens com energia, um por vez na mão (a porta de todo lado, o LED aceso na frente), e os cinco cubos postos com a energia pela metade (o núcleo girando)."""
    sky()
    h.command("tick unfreeze")
    h.command(CLEAR)
    h.command("clear @s")
    for slot, (tier, energy) in enumerate(ITENS):
        h.command(f"item replace entity @s hotbar.{slot} with mekanism:{tier}_energy_cube[mekanism:energy={energy}L]")
    h.command("tp @s 62.5 -59 78.5 0 0")
    h.ctl("key", "1", "tap")
    time.sleep(4.0)
    for slot in range(1, 5):
        h.ctl("key", str(slot + 1), "tap")
        time.sleep(2.5)
    h.ctl("key", "1", "tap")
    x, y, z = BASE
    for i, (tier, _) in enumerate(ITENS):
        h.command(f"setblock {x + i} {y} {z} mekanism:{tier}_energy_cube[facing=north]")
    time.sleep(1.5)
    for i, (tier, energy) in enumerate(ITENS):
        if tier == "creative":
            h.command(f"data merge block {x + i} {y} {z} {{energy_amount:{energy}L,component_config:{{config0:[I;4,4,4,4,4,4]}}}}")
        else:
            h.command(f"data merge block {x + i} {y} {z} {{energy:{energy // 2},component_config:{{config0:[I;4,1,1,1,1,1]}}}}")
    h.command("tp @s " + TP)
    time.sleep(9.0)
    h.command(CLEAR)
    h.command("clear @s")


def congelar():
    sky()
    h.command("tick unfreeze")
    h.command("tp @s " + TP)
    h.command(CLEAR)
    place()
    time.sleep(2.0)
    h.command("tick freeze")
    # O chat some depois de 10 s: com ele no quadro, os prints diferem pelo texto que apaga e não pelo núcleo.
    time.sleep(14.0)
    for i in range(3):
        shot(f"congelar_{i}")
        time.sleep(1.5)
    print("gametime", " ".join(h.command("time query gametime")))


if __name__ == "__main__":
    modo = sys.argv[1]
    SHOTS = SHOTS if len(sys.argv) < 3 else sys.argv[2]
    try:
        {"repor": repor, "congelar": congelar, "video": video}[modo]()
    finally:
        restore()
