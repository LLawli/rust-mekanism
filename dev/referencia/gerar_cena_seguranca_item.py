#!/usr/bin/env python3
"""Escreve `cenas/seguranca_itens.json`: o tooltip de cada uma das quatro máquinas e do Heat Generator (frente `seguranca`, decisão 253), no inventário, com o item sem dono
(`Security: Public` com a tecla de detalhes), com dono e modo `private` e `trusted` (`mekanism:owner` e `mekanism:security`, o dono é um UUID fixo que nenhum cliente conhece, então os dois
jogos escrevem o mesmo texto de reserva), com e sem a tecla de detalhes. O mesmo arquivo roda no original (`oficial_item.sh CenaItem.java seguranca_itens.json PASTA`) e no port
(`ROTEIRO=seguranca_itens.json horizon2/h2_blocos.py cena PASTA`); `comparar_item.py` compara o recorte `detalhe`.
"""
import json
import os

ITENS = ["mekanism:enrichment_chamber", "mekanism:crusher", "mekanism:energized_smelter", "mekanism:metallurgic_infuser", "mekanismgenerators:heat_generator"]
DONO = "[I;1414745163,-166969119,-1858692902,1358730404]"
# (nome, componentes do item, tecla)
VARIANTES = [
    ("publico_detalhes", "", "detalhes"),
    ("privado_nenhuma", f"[mekanism:owner={DONO},mekanism:security=\"private\"]", "nenhuma"),
    ("privado_detalhes", f"[mekanism:owner={DONO},mekanism:security=\"private\"]", "detalhes"),
    ("confiavel_detalhes", f"[mekanism:owner={DONO},mekanism:security=\"trusted\"]", "detalhes"),
]
X, Y, Z = 60, -60, 80


def shots():
    out = []
    stand = f"{X + 0.5} {Y} {Z + 3.5} 180 20"
    for item in ITENS:
        short = item.split(":")[1]
        for nome, componentes, tecla in VARIANTES:
            cmds = ["#tela", "#tecla nenhuma", "clear @a", f"item replace entity @a hotbar.0 with {item}{componentes}", "#esperar 10", f"tp @a {stand}", "#esperar 10"]
            cmds += ["#inventario 0", f"#tecla {tecla}", "#esperar 10"]
            out.append(dict(name=f"detalhe_seg_{short}_{nome}", cmds=cmds, tp=stand))
    out.append(dict(name="fim_seguranca", cmds=["#tela", "#tecla nenhuma", "clear @a", "#esperar 5"], tp=stand))
    return out


def main():
    scene = dict(
        hud=True,
        setup=["difficulty peaceful", "time set noon", "weather clear", "gamemode survival @a", "gamerule advance_time false", "gamerule advance_weather false",
               "fill 52 -61 70 100 -61 90 minecraft:stone", "fill 52 -60 70 100 -52 90 minecraft:air", "kill @e[type=minecraft:item]"],
        clear="weather clear",
        shots=shots(),
    )
    path = os.path.join(os.path.dirname(os.path.abspath(__file__)), "cenas", "seguranca_itens.json")
    with open(path, "w") as out:
        json.dump(scene, out, indent=1)
    print(path, len(scene["shots"]), "tiros")


if __name__ == "__main__":
    main()
