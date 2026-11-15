#!/usr/bin/env python3
"""Em jogo (container `horizon2`, H_NAME=horizon2): o vídeo da tela do Heat Generator enchendo de lava (frente `maquina-sobras`, decisão 243).

    h2_lava_video.py

O Heat Generator posto com a lava em 500, 1.000 e 5.000 mB (o chão de pedra e o ar em volta primeiro, porque o gerador puxa a lava dos vizinhos (a caverna do mundo `limpo` tem lava ao lado); o item da cena `cenas/heat_lava.json`, com carvão para o tanque não ser queimado: sem combustível o gerador gasta a lava, e um `data merge` com a tela aberta não muda o tanque que o tile já leu). O último nível passa da capacidade (decisão 243). Grave com `gravar-jogo rec <ws> lava.mp4 50 <container>` aberto uns 2 s antes.
"""
import json, os, subprocess, sys, time

TOOLS = os.path.dirname(os.path.abspath(__file__))
os.environ.setdefault("H_NAME", "horizon2")
sys.path.insert(0, TOOLS)
import h2_blocos as hb  # noqa: E402
sys.path.insert(0, os.path.join(TOOLS, ".."))
from gerar_cena_lava import componentes  # noqa: E402

ITEM = "mekanismgenerators:heat_generator"
X, Y, Z = 60, -60, 80
NIVEIS = [500, 1000, 5000]


def ctl(*args):
    return subprocess.run([f"{TOOLS}/ctl.sh", *args], capture_output=True, text=True, timeout=180).stdout


def cmd(text):
    try:
        return " ".join(json.loads(ctl("command", text))["chat"])
    except (json.JSONDecodeError, KeyError):
        return ""


def main():
    stand = f"{X + 0.5} {Y} {Z + 3.5} 180 20"
    ctl("wait-ready")
    for c in ("gamemode survival", "weather clear", "time set noon", "gamerule advance_time false", "gamerule advance_weather false", "gamerule spawn_mobs false",
              "kill @e[type=!minecraft:player]", f"fill {X - 8} {Y - 1} {Z - 8} {X + 8} {Y - 1} {Z + 8} minecraft:stone", f"fill {X - 8} {Y} {Z - 8} {X + 8} {Y + 6} {Z + 8} minecraft:air", "clear @s"):
        cmd(c)
    ctl("options-set", "guiScale", "4")
    ctl("options-set", "chatVisibility", "2")
    cmd(f"tp @s {stand}")
    time.sleep(1.0)
    # O tile só lê o tanque do NBT ao nascer (um `data merge` com a tela aberta não o muda): cada nível é o item do Heat Generator com a lava naquela quantidade, posto de novo.
    for quantidade in NIVEIS:
        item, itens = componentes(quantidade)
        hb.item_cena(f"item replace entity @a hotbar.0 with {item}[{itens}]")
        hb.item_cena(f"#colocar {X} {Y - 1} {Z}")
        time.sleep(1.5)
        hb.item_cena(f"#abrir {X} {Y} {Z}")
        time.sleep(2.0)
        cmd("playsound minecraft:item.bucket.empty_lava player @s ~ ~ ~ 1")
        time.sleep(1.8)
        hb.item_cena("#tela")
        hb.item_cena(f"#quebrar {X} {Y} {Z}")
        time.sleep(1.0)
        cmd("kill @e[type=minecraft:item]")
    cmd("clear @s")


main()
