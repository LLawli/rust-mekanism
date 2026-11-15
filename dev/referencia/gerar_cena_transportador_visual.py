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


COMPONENTES = ["minecraft:diamond_sword[enchantments={sharpness:5}]", "minecraft:iron_pickaxe[damage=100]", "minecraft:stick[custom_name='Nome']", "minecraft:enchanted_book", "minecraft:diamond_sword[enchantments={sharpness:5}]",
               "minecraft:iron_pickaxe[damage=100]", "minecraft:golden_apple", "minecraft:apple"]


def linha(x0, z0, cor, itens=ITENS):
    """Um baú, três transportadores em x, a curva para o sul, mais dois, a curva para o leste e outros dois, e o baú de chegada."""
    caminho = [(x0 + 1, z0), (x0 + 2, z0), (x0 + 3, z0), (x0 + 3, z0 + 1), (x0 + 3, z0 + 2), (x0 + 4, z0 + 2), (x0 + 5, z0 + 2)]
    cmds = [f"setblock {x0} {Y} {z0} minecraft:chest"]
    cmds += [f"setblock {x} {Y} {z} mekanism:basic_logistical_transporter" for x, z in caminho]
    cmds.append(f"setblock {x0 + 6} {Y} {z0 + 2} minecraft:chest")
    cmds += [f"item replace block {x0} {Y} {z0} container.{i} with {item} 16" for i, item in enumerate(itens)]
    cmds.append("#esperar 20")
    cmds += [f"#cor {x} {Y} {z} {cor}" for x, z in caminho]
    cmds.append("#esperar 10")
    return cmds, caminho[0]


# O vidro das 18 `EnumColor`: três transportadores por tiro (a 6,5 blocos de altura a janela mostra uns 8 blocos de largura, e o `tp` fica no do meio).
VIDRO_Z = 70
VIDRO_X0 = 52
POR_TIRO = 3
# Nada a limpar entre os tiros (o `clear` do roteiro apaga o que o tiro anterior montou: um bloco de ar longe da cena).
SEM_LIMPEZA = "fill 52 -60 140 52 -60 140 minecraft:air"
# A fila de passos: depois de `PASSOS_BASE` ticks com o servidor parado, um tiro a cada `tick step 1`. Os dois jogos não começam a puxar no mesmo tick, e o tiro de cada passo deixa achar,
# a posteriori, o par (passo do original, passo do port) em que as pilhas estão no mesmo lugar.
PASSOS_BASE = 30
TIROS_DE_PASSO = 12


def tiros_de_passo(prefixo, tp):
    return [{"name": f"{prefixo}_{k:02d}", "cmds": ["tick step 1", "#esperar 8"], "tp": tp} for k in range(TIROS_DE_PASSO)]


def main():
    cmds_a, primeiro_a = linha(60, Z, 4)
    base_a = cmds_a + ["tick freeze", f"#conexao {primeiro_a[0]} {Y} {primeiro_a[1]} west=pull", f"tick step {PASSOS_BASE}", f"#esperar {PASSOS_BASE * 2 + 20}"]
    cmds_c, primeiro_c = linha(80, Z, 4, COMPONENTES)
    base_c = ["tick unfreeze"] + cmds_c + ["tick freeze", f"#conexao {primeiro_c[0]} {Y} {primeiro_c[1]} west=pull", f"tick step {PASSOS_BASE}", f"#esperar {PASSOS_BASE * 2 + 20}"]
    tp_a = "63.5 -51.5 83.0 0 90"
    tp_c = "83.5 -51.5 83.0 0 90"
    vidro_tiros = []
    n = len(range(18))
    for j in range(n // POR_TIRO):
        cmds = []
        if j == 0:
            cmds += [f"setblock {VIDRO_X0 + 2 * k} {Y} {VIDRO_Z} mekanism:basic_logistical_transporter" for k in range(n)] + ["#esperar 20"]
            cmds += [f"#cor {VIDRO_X0 + 2 * k} {Y} {VIDRO_Z} {k}" for k in range(n)] + ["#esperar 20"]
        meio = VIDRO_X0 + 2 * (POR_TIRO * j + 1) + 0.5
        vidro_tiros.append({"name": f"transportador_vidro_{j}", "cmds": cmds, "tp": f"{meio} -53.5 {VIDRO_Z + 0.5} 0 90"})
    roteiro = {
        "setup": SETUP,
        "clear": SEM_LIMPEZA,
        "shots": vidro_tiros
        + [{"name": "pilhas_base", "cmds": base_a, "tp": tp_a}] + tiros_de_passo("pilhas", tp_a)
        + [{"name": "componentes_base", "cmds": base_c, "tp": tp_c}] + tiros_de_passo("componentes", tp_c),
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
        "shots": [
            {"name": "transportador_hotbar", "cmds": ["#esperar 10"], "tp": "60.0 -58.0 70.0 0 10"},
            # os quatro tiers: o primeiro tiro mostra basic, advanced e elite dos três tipos (nove), este o ultimate deles (os doze itens)
            {"name": "transportador_hotbar_ultimate", "cmds": [f"item replace entity @a hotbar.{i} with mekanism:ultimate_{kind} 1" for i, kind in enumerate(kinds)] + ["#esperar 10"],
             "tp": "60.0 -58.0 70.0 0 10"},
        ],
    }
    with open(os.path.join(HERE, "cenas/transportador_hotbar.json"), "w") as handle:
        json.dump(hotbar, handle, indent=1)
        handle.write("\n")


if __name__ == "__main__":
    main()
