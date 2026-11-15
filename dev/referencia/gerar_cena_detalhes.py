#!/usr/bin/env python3
"""Escreve `cenas/detalhes_itens.json`: o tooltip de cada item com `DETAILS` que o port já mostra (as quatro máquinas, o Heat Generator, os cubos, os geradores, as fábricas e uns blocos sem
lógica), sem tecla, com a tecla de detalhes (Left Shift) e com Shift mais N, no inventário. O mesmo arquivo roda no original (`oficial_item.sh CenaItem.java detalhes_itens.json PASTA`, que
tem o `#tecla`) e no port (`ROTEIRO=detalhes_itens.json horizon2/h2_blocos.py cena PASTA`); `comparar_item.py` compara o recorte `detalhe` (a caixa do tooltip e o que ela pode subir).
"""
import json
import os

ITENS = [
    "mekanism:enrichment_chamber", "mekanism:crusher", "mekanism:energized_smelter", "mekanism:metallurgic_infuser", "mekanismgenerators:heat_generator",
    "mekanism:basic_energy_cube", "mekanism:creative_energy_cube", "mekanism:personal_chest", "mekanism:security_desk", "mekanism:industrial_alarm",
    "mekanism:logistical_sorter", "mekanism:digital_miner", "mekanism:teleporter", "mekanism:basic_smelting_factory", "mekanism:ultimate_infusing_factory",
    "mekanism:laser", "mekanism:qio_dashboard", "mekanismgenerators:solar_generator", "mekanismgenerators:wind_generator", "mekanismgenerators:fission_reactor_logic_adapter",
]
TECLAS = ["nenhuma", "detalhes", "descricao"]
X, Y, Z = 60, -60, 80


def shots():
    out = []
    stand = f"{X + 0.5} {Y} {Z + 3.5} 180 20"
    for item in ITENS:
        short = item.split(":")[1]
        for index, tecla in enumerate(TECLAS):
            cmds = ["#tela", "#tecla nenhuma"]
            if index == 0:
                cmds += ["clear @a", f"item replace entity @a hotbar.0 with {item}", "#esperar 10", f"tp @a {stand}", "#esperar 10"]
            # A tecla vem depois de abrir o inventário: o cliente do original solta as teclas ao abrir uma tela (medido: com `#tecla` antes, os três estados saíam iguais).
            cmds += ["#inventario 0", f"#tecla {tecla}", "#esperar 10"]
            out.append(dict(name=f"detalhe_{short}_{tecla}", cmds=cmds, tp=stand))
    out.append(dict(name="fim_detalhes", cmds=["#tela", "#tecla nenhuma", "clear @a", "#esperar 5"], tp=stand))
    return out


def main():
    scene = dict(
        hud=True,
        setup=["difficulty peaceful", "time set noon", "weather clear", "gamemode survival @a", "gamerule advance_time false", "gamerule advance_weather false",
               "fill 52 -61 70 100 -61 90 minecraft:stone", "fill 52 -60 70 100 -52 90 minecraft:air", "kill @e[type=minecraft:item]"],
        clear="weather clear",
        shots=shots(),
    )
    path = os.path.join(os.path.dirname(os.path.abspath(__file__)), "cenas", "detalhes_itens.json")
    with open(path, "w") as out:
        json.dump(scene, out, indent=1)
    print(path, len(scene["shots"]), "tiros")


if __name__ == "__main__":
    main()
