#!/usr/bin/env python3
"""Escreve `cenas/transportador_visual.json` e `cenas/transportador_hotbar.json`, os roteiros da frente `transportador-visual` (Fase 4): o original (`testes/CenaBlocos.java`,
`oficial_blocos.sh`) e o port (`horizon2/h2_blocos.py`) rodam o mesmo roteiro e tiram os mesmos prints.

`transportador_linha`: duas linhas de transportadores logísticos com curvas, cada uma de um baú cheio a outro baú, uma de cada cor (`#cor X Y Z ORDINAL`, o `EnumColor` do transportador: o
original o põe por `LogisticalTransporter.setColor`, o port pelo clique do Configurator agachado no centro, que passa à cor seguinte). O servidor para (`tick freeze`) antes de o primeiro
transportador de cada linha puxar do baú (`#conexao`, `PULL`), anda `PASSOS` ticks (`tick step`) e fica parado: os dois jogos mostram as pilhas no mesmo tick. `transportador_vidro`: seis
transportadores de seis `EnumColor` (a tinta do vidro de cada cor).
`transportador_hotbar` (`hud`): os nove itens que o `TransmitterTypeDecorator` enfeita, na barra de atalhos."""
import json
import os

HERE = os.path.dirname(os.path.abspath(__file__))
Y = -60
Z = 82
PASSOS = 47
# O vidro de seis das 18 `EnumColor` (o clique do Configurator no port passa de cor em cor, então as de ordinal baixo saem mais rápido): preta, azul, vermelha escura, vermelha, branca e rosa.
CORES_DO_VIDRO = [0, 1, 4, 12, 15, 17]
ITENS = ["minecraft:cobblestone", "minecraft:redstone", "minecraft:diamond", "minecraft:stick", "minecraft:oak_log", "minecraft:coal", "minecraft:apple", "minecraft:iron_ingot"]
SETUP = ["difficulty peaceful", "time set 6000", "weather clear", "gamemode creative", "fill 52 -60 52 75 -52 90 minecraft:air", "fill 76 -60 52 99 -52 90 minecraft:air",
         "fill 100 -60 52 123 -52 90 minecraft:air", "fill 20 -60 91 160 -50 110 minecraft:air", "fill 20 -60 111 160 -50 130 minecraft:air"]
CLEAR = "fill 52 -60 78 120 -52 96 minecraft:air"


def linha(x0, z0, cor):
    """Um baú, três transportadores em x, a curva para o sul, mais dois, a curva para o leste e outros dois, e o baú de chegada."""
    caminho = [(x0 + 1, z0), (x0 + 2, z0), (x0 + 3, z0), (x0 + 3, z0 + 1), (x0 + 3, z0 + 2), (x0 + 4, z0 + 2), (x0 + 5, z0 + 2)]
    cmds = [f"setblock {x0} {Y} {z0} minecraft:chest"]
    cmds += [f"setblock {x} {Y} {z} mekanism:basic_logistical_transporter" for x, z in caminho]
    cmds.append(f"setblock {x0 + 6} {Y} {z0 + 2} minecraft:chest")
    cmds += [f"item replace block {x0} {Y} {z0} container.{i} with {item} 16" for i, item in enumerate(ITENS)]
    cmds.append("#esperar 20")
    cmds += [f"#cor {x} {Y} {z} {cor}" for x, z in caminho]
    cmds.append("#esperar 10")
    return cmds, caminho[0]


def main():
    cmds_a, primeiro_a = linha(60, Z, 4)
    cmds_b, primeiro_b = linha(70, Z, 1)
    linha_cmds = cmds_a + cmds_b + ["tick freeze", f"#conexao {primeiro_a[0]} {Y} {primeiro_a[1]} west=pull", f"#conexao {primeiro_b[0]} {Y} {primeiro_b[1]} west=pull", f"tick step {PASSOS}", f"#esperar {PASSOS * 2 + 20}"]
    vidro = []
    for k in range(len(CORES_DO_VIDRO)):
        vidro += [f"setblock {56 + 2 * k} {Y} {Z} mekanism:basic_logistical_transporter"]
    vidro.append("#esperar 20")
    for k, cor in enumerate(CORES_DO_VIDRO):
        vidro.append(f"#cor {56 + 2 * k} {Y} {Z} {cor}")
    vidro.append("#esperar 20")
    roteiro = {
        "setup": SETUP,
        "clear": CLEAR,
        "shots": [
            {"name": "transportador_linha", "cmds": linha_cmds, "tp": "68.0 -50.0 83.0 0 90"},
            {"name": "transportador_vidro", "cmds": ["tick unfreeze"] + vidro, "tp": "61.0 -53.5 82.5 0 90"},
        ],
    }
    with open(os.path.join(HERE, "cenas/transportador_visual.json"), "w") as handle:
        json.dump(roteiro, handle, indent=1)
        handle.write("\n")
    tiers = ("basic", "advanced", "elite", "ultimate")
    kinds = ("universal_cable", "pressurized_tube", "thermodynamic_conductor")
    itens = [f"mekanism:{tier}_{kind}" for kind in kinds for tier in tiers[:3]]
    hotbar = {
        "setup": SETUP + ["clear @a"] + [f"item replace entity @a hotbar.{i} with {item} 1" for i, item in enumerate(itens)],
        "clear": "fill 52 -60 78 56 -52 80 minecraft:air",
        "hud": True,
        "shots": [{"name": "transportador_hotbar", "cmds": ["#esperar 10"], "tp": "60.0 -58.0 70.0 0 10"}],
    }
    with open(os.path.join(HERE, "cenas/transportador_hotbar.json"), "w") as handle:
        json.dump(hotbar, handle, indent=1)
        handle.write("\n")


if __name__ == "__main__":
    main()
