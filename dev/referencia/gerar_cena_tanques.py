#!/usr/bin/env python3
"""Escreve o roteiro da cena dos Fluid Tanks (frente `tanques`, 6.ª rodada): `cenas/tanques6.json` (o original, `oficial_blocos.sh CenaBlocos.java tanques6.json PASTA`) e
`cenas/tanques6_port.json` (o port, `ROTEIRO=tanques6_port.json H_NAME=horizon horizon2/h2_blocos.py cena PASTA`). Os dois só diferem em como o tanque recebe o fluido: o
original pelo NBT dele (`fluid_tanks`), o port pela diretiva `#fluido` (o NBT `machinery_basis.tanks`).

Cinco tanques em fila (x 59 a 63, z 82), sobre uma plataforma de `smooth_stone`, vistos pela mesma câmera. Os tiros parados esperam a escala animada chegar (220 ticks); os dois que medem a animação e a
válvula param o servidor (`tick freeze`), põem o fluido e andam um número exato de ticks (`tick step`), para os dois jogos tirarem o print no mesmo tick do tile.
"""
import json
import os

XS = (59, 60, 61, 62, 63)
Z = 82
# Os dois jogos montam a cena sobre uma plataforma de `smooth_stone` (um modelo só, sem variante sorteada pela posição): o chão do mundo do original (`GameTestWorld`,
# plano, pedra em y -61) e o dos mundos de teste do port (que não são planos) não dariam o mesmo fundo atrás do vidro dos tanques. No original a plataforma fica em y -60,
# em cima do chão; no port, em y 140, no ar. O último tiro (`limpeza`) a tira.
Y_ORIGINAL, Y_PORT = -59, 141
PLATAFORMA = "fill 45 %d 70 77 %d 110 %s"


def fluido(port, x, y, fluid, amount, extra=""):
    if port:
        cmds = ["#fluido %d %d %d %s %d" % (x, y, Z, fluid, amount)]
        if extra:
            cmds.append("data merge block %d %d %d {%s}" % (x, y, Z, extra))
        return cmds
    extra = "," + extra if extra else ""
    return ['data merge block %d %d %d {fluid_tanks:[{container:0,stored:{amount:%dL,type:{id:"%s"}}}]%s}' % (x, y, Z, amount, fluid, extra)]


def postos(Y, tier="basic"):
    return ["tick unfreeze"] + ["setblock %d %d %d mekanism:%s_fluid_tank" % (x, Y, Z, tier) for x in XS] + ["#esperar 30"]


def item(tier, fluid, amount):
    return 'mekanism:%s_fluid_tank[mekanism:fluids={containers:[{type:{id:"%s"},amount:%dL}]}]' % (tier, fluid, amount)


def roteiro(port):
    Y = Y_PORT if port else Y_ORIGINAL
    CAMERA = "61.5 %.1f 78.6 0 27" % (Y + 0.6)
    MAO = "62.5 %d 78.5 0 0" % (Y + 1)
    shots = []
    shots.append({"name": "vazios", "cmds": ["clear @a"] + postos(Y) + ["#esperar 40"], "tp": CAMERA})
    cmds = postos(Y)
    for x, amount in zip(XS, (1000, 8000, 16000, 24000, 32000)):
        cmds += fluido(port, x, Y, "minecraft:water", amount)
    shots.append({"name": "agua_niveis", "cmds": cmds + ["#esperar 240"], "tp": CAMERA})
    cmds = postos(Y)
    for x, (fluid, amount) in zip(XS, (("minecraft:lava", 16000), ("mekanism:heavy_water", 16000), ("mekanism:steam", 8000), ("mekanism:hydrogen", 24000), ("mekanism:brine", 32000))):
        cmds += fluido(port, x, Y, fluid, amount)
    shots.append({"name": "fluidos", "cmds": cmds + ["#esperar 240"], "tp": CAMERA})
    # A escala animando: 10 ticks depois de o tanque receber o fluido, com o servidor parado antes e depois.
    cmds = postos(Y) + ["tick freeze", "#esperar 10"]
    for x, amount in zip(XS, (32000, 16000, 32000, 8000, 32000)):
        cmds += fluido(port, x, Y, "minecraft:water", amount)
    shots.append({"name": "animando", "cmds": cmds + ["#esperar 10", "tick step 10", "#esperar 60"], "tp": CAMERA})
    # A válvula: um tanque cheio e ativo em cima de um vazio; 6 ticks depois o de baixo tem a coluna que desce do teto.
    cmds = postos(Y, "ultimate") + ["setblock %d %d %d mekanism:basic_fluid_tank" % (x, Y + 1, Z) for x in (59, 61, 63)] + ["#esperar 30", "tick freeze", "#esperar 10"]
    for x, fluid in zip((59, 61, 63), ("minecraft:water", "minecraft:lava", "mekanism:heavy_water")):
        cmds += fluido(port, x, Y + 1, fluid, 32000, "active_state:1b")
    shots.append({"name": "valvula", "cmds": cmds + ["#esperar 10", "tick step 6", "#esperar 60"], "tp": CAMERA})
    # O item na mão e na barra: metade de água na mão, e na barra a lava cheia, a água pesada, o vapor e um vazio.
    barra = [item("basic", "minecraft:water", 16000), item("basic", "minecraft:lava", 32000), item("advanced", "mekanism:heavy_water", 32000),
             item("ultimate", "mekanism:steam", 64000), "mekanism:elite_fluid_tank"]
    cmds = ["tick unfreeze", "clear @a", "#esperar 20"] + ["item replace entity @a hotbar.%d with %s" % (i, stack) for i, stack in enumerate(barra)] + ["#esperar 40"]
    shots.append({"name": "item_agua", "cmds": cmds, "tp": MAO})
    barra = [barra[1], barra[0]] + barra[2:]
    cmds = ["clear @a", "#esperar 20"] + ["item replace entity @a hotbar.%d with %s" % (i, stack) for i, stack in enumerate(barra)] + ["#esperar 40"]
    shots.append({"name": "item_lava", "cmds": cmds, "tp": MAO})
    shots.append({"name": "limpeza", "cmds": ["tick unfreeze", "clear @a", PLATAFORMA % (Y - 1, Y - 1, "minecraft:air"), "#esperar 10"], "tp": MAO})
    return {
        "setup": ["difficulty peaceful", "time set 6000", "weather clear", "gamemode creative", PLATAFORMA % (Y - 1, Y - 1, "minecraft:smooth_stone"),
                  "fill 52 %d 76 70 %d 88 minecraft:air" % (Y, Y + 5)],
        "clear": "fill 55 %d 80 67 %d 84 minecraft:air" % (Y, Y + 3),
        "hud": True,
        "shots": shots,
    }


if __name__ == "__main__":
    pasta = os.path.join(os.path.dirname(os.path.abspath(__file__)), "cenas")
    for nome, port in (("tanques6.json", False), ("tanques6_port.json", True)):
        with open(os.path.join(pasta, nome), "w") as arquivo:
            json.dump(roteiro(port), arquivo, indent=1, ensure_ascii=False)
            arquivo.write("\n")
