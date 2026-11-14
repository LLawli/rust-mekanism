#!/usr/bin/env python3
"""Escreve `cenas/item_conteudo.json`, o roteiro do original (`testes/CenaItem.java`, por `oficial_blocos.sh CenaItem.java item_conteudo.json PASTA`, que o chama de `cena-blocos.json`
no `oficial_item.sh`) e que o port repete em `horizon2/h2_item_conteudo.py cena`. Cada máquina recebe o item com os componentes de `h2_item_conteudo.MACHINES` e passa por: o tooltip no
inventário, posta, a tela aberta, a aba de upgrades, quebrada, o item pego do chão pela mão, o tooltip de novo, posta de novo e a tela de novo. Os nomes dos tiros são os mesmos nos dois lados.
"""
import json
import os
import sys

sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), "horizon2"))
from item_conteudo_dados import MACHINES  # noqa: E402

Y = -60
Z = 80
ABA = "190,19"


def spot(index):
    return 60 + 4 * index, Y, Z


def shots():
    out = []
    for index, (item, _pos, components, _label, _total) in enumerate(MACHINES):
        short = item.split(":")[1]
        x, y, z = spot(index)
        stand = f"{x + 0.5} {y} {z + 3.5} 180 20"
        place = f"#colocar {x} {y - 1} {z}"
        open_gui = f"#abrir {x} {y} {z}"
        out += [
            dict(name=f"tooltip_{short}", cmds=["#tela", "clear @a", "kill @e[type=minecraft:item]", f"item replace entity @a hotbar.0 with {item}[{components}]", "#esperar 10", f"tp @a {stand}", "#esperar 10", "#inventario 0"], tp=stand),
            dict(name=f"tela_{short}", cmds=["#tela", "#esperar 5", place, "#esperar 20", open_gui, "#esperar 30"], tp=stand),
            dict(name=f"aba_{short}", cmds=[f"#clicar {ABA}", "#esperar 10"], tp=stand),
            dict(name=f"quebrada_{short}", cmds=["#tela", "#esperar 5", f"#quebrar {x} {y} {z}", "#esperar 5"], tp=stand),
            dict(name=f"tooltip_depois_{short}", cmds=["execute as @a at @e[type=minecraft:item,limit=1] run tp @a ~ ~ ~", "#esperar 30", f"tp @a {stand}", "#esperar 10", "#inventario 0"], tp=stand),
            dict(name=f"tela_depois_{short}", cmds=["#tela", "#esperar 5", place, "#esperar 20", open_gui, "#esperar 30"], tp=stand),
            dict(name=f"fim_{short}", cmds=["#tela", f"#quebrar {x} {y} {z}", "#esperar 10", "kill @e[type=minecraft:item]", "clear @a", "#esperar 5"], tp=stand),
        ]
    return out


def main():
    scene = dict(
        hud=True,
        setup=["difficulty peaceful", "time set noon", "weather clear", "gamemode survival @a", "gamerule advance_time false", "gamerule advance_weather false",
               "fill 52 -60 70 100 -52 90 minecraft:air", "kill @e[type=minecraft:item]"],
        clear="weather clear",
        shots=shots(),
    )
    path = os.path.join(os.path.dirname(os.path.abspath(__file__)), "cenas", "item_conteudo.json")
    with open(path, "w") as out:
        json.dump(scene, out, indent=1)
    print(path, len(scene["shots"]), "tiros")


if __name__ == "__main__":
    main()
