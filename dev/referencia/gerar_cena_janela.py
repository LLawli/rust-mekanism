#!/usr/bin/env python3
"""Escreve `cenas/upgrade_janela.json`: a janela de upgrades da Enrichment Chamber (aba de upgrades aberta) com cada conjunto de tipos instalados (`CONJUNTOS`), posta de um item com os
upgrades e a tela aberta, os mesmos tiros nos dois jogos (`oficial_item.sh` no original, `h2_blocos.py cena` no port). A ordem dos tipos na janela é a do mapa do fastutil, que depende do
`System.identityHashCode` sorteado a cada execução do JVM: a de uma execução do original e a de uma do port só coincidem às vezes, então a conferência pixel a pixel vale para os conjuntos
em que as duas ordens saíram iguais (um tipo só sempre) e para os outros o que difere é a ordem (`comparar_item.py`, o recorte `aba`). `gerar_cena_janela.py` não toma argumentos.
"""
import json
import os

Y = -60
Z = 80
ABA = "190,19"
TIPOS = {"speed": 3, "energy": 2, "muffling": 1}
CONJUNTOS = [
    ("speed", ["speed"]),
    ("energy", ["energy"]),
    ("muffling", ["muffling"]),
    ("speed_energy", ["speed", "energy"]),
    ("speed_muffling", ["speed", "muffling"]),
    ("energy_muffling", ["energy", "muffling"]),
    ("speed_energy_muffling", ["speed", "energy", "muffling"]),
]


def componentes(tipos):
    mapa = ",".join(f'"mekanism:{t}":{TIPOS[t]}' for t in tipos)
    return f'mekanism:upgrades={{upgrades:{{{mapa}}},input:{{}},output:{{}}}},mekanism:redstone_control="high",mekanism:energy=200L'


def shots():
    out = []
    for index, (nome, tipos) in enumerate(CONJUNTOS):
        x = 60 + 4 * index
        stand = f"{x + 0.5} {Y} {Z + 3.5} 180 20"
        place = f"#colocar {x} {Y - 1} {Z}"
        open_gui = f"#abrir {x} {Y} {Z}"
        out += [
            dict(name=f"aba_{nome}", cmds=["#tela", "clear @a", "kill @e[type=minecraft:item]", f"item replace entity @a hotbar.0 with mekanism:enrichment_chamber[{componentes(tipos)}]", "#esperar 10",
                                          f"tp @a {stand}", "#esperar 10", place, "#esperar 20", open_gui, "#esperar 30", f"#clicar {ABA}", "#esperar 10"], tp=stand),
            dict(name=f"fim_{nome}", cmds=["#tela", f"#quebrar {x} {Y} {Z}", "#esperar 10", "kill @e[type=minecraft:item]", "clear @a", "#esperar 5"], tp=stand),
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
    path = os.path.join(os.path.dirname(os.path.abspath(__file__)), "cenas", "upgrade_janela.json")
    with open(path, "w") as out:
        json.dump(scene, out, indent=1)
    print(path, len(scene["shots"]), "tiros")


if __name__ == "__main__":
    main()
