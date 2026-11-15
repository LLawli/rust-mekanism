#!/usr/bin/env python3
"""Escreve `cenas/heat_lava.json`: o Heat Generator com o tanque de lava vazio, pela metade e cheio, posto e com a tela aberta (os mesmos tiros nos dois jogos; o roteiro do original e o do
port são o mesmo arquivo). O recorte do medidor de lava e a tela inteira saem de `comparar_lava.py`.
"""
import json
import os
import re
import sys

sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), "horizon2"))
from item_conteudo_dados import MACHINES  # noqa: E402

Y = -60
Z = 80
NIVEIS = [("vazia", 0), ("meia", 500), ("cheia", 1000), ("sobre", 5000), ("carvao", 500)]


def componentes(total, carvao=False):
    item, _pos, components, _label, _total = [m for m in MACHINES if m[0].endswith("heat_generator")][0]
    if total == 0:
        components = re.sub(r'mekanism:fluids=\{containers:\[\{type:\{id:"minecraft:lava"\},amount:\d+L\}\]\},', "", components)
    else:
        components = re.sub(r"amount:1500L", f"amount:{total}L", components)
    # O carvão do item vira lava no tick (`FluidFuelInventorySlot.fillOrBurn`, 80 mB por carvão, enquanto houver espaço no tanque): com ele, 0 e 500 mB chegam a 1.000 mB
    # antes do print, nos dois jogos. Sem carvão o nível do tiro é o que o tanque guarda.
    if not carvao:
        components = components.replace('{type:{id:"minecraft:coal"},amount:17L},', "")
    return item, components


def shots():
    out = []
    for index, (nome, total) in enumerate(NIVEIS):
        item, components = componentes(total, nome == "carvao")
        x, y, z = 60 + 4 * index, Y, Z
        stand = f"{x + 0.5} {y} {z + 3.5} 180 20"
        place = f"#colocar {x} {y - 1} {z}"
        out += [
            dict(name=f"lava_{nome}", cmds=["#tela", "clear @a", "kill @e[type=minecraft:item]", f"item replace entity @a hotbar.0 with {item}[{components}]", "#esperar 10", f"tp @a {stand}", "#esperar 10", place, "#esperar 20", f"#abrir {x} {y} {z}", "#esperar 30"], tp=stand),
            dict(name=f"fim_lava_{nome}", cmds=["#tela", f"#quebrar {x} {y} {z}", "#esperar 10", "kill @e[type=minecraft:item]", "clear @a", "#esperar 5"], tp=stand),
        ]
    return out


def main():
    scene = dict(
        hud=True,
        setup=["difficulty peaceful", "time set noon", "weather clear", "gamemode survival @a", "gamerule advance_time false", "gamerule advance_weather false",
               "fill 52 -61 70 100 -61 90 minecraft:stone", "fill 52 -60 70 100 -52 90 minecraft:air", "kill @e[type=minecraft:item]"],
        clear="weather clear",
        shots=shots(),
    )
    path = os.path.join(os.path.dirname(os.path.abspath(__file__)), "cenas", "heat_lava.json")
    with open(path, "w") as out:
        json.dump(scene, out, indent=1)
    print(path, len(scene["shots"]), "tiros")


if __name__ == "__main__":
    main()
