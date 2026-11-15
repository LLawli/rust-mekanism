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
SETUP = ["difficulty peaceful", "time set 6000", "weather clear", "gamemode creative", "attribute @a minecraft:gravity base set 0", "fill 52 -60 52 75 -52 90 minecraft:air", "fill 76 -60 52 99 -52 90 minecraft:air",
         "fill 100 -60 52 123 -52 90 minecraft:air", "fill 20 -60 91 160 -50 110 minecraft:air", "fill 20 -60 111 160 -50 130 minecraft:air", "fill 20 -60 131 160 -50 150 minecraft:air", "fill 20 -61 52 160 -61 130 minecraft:stone"]
CLEAR = "fill 52 -60 78 120 -52 96 minecraft:air"


COMPONENTES = ["minecraft:diamond_sword[enchantments={sharpness:5}]", "minecraft:iron_pickaxe[damage=100]", "minecraft:stick[custom_name='Nome']", "minecraft:enchanted_book", "minecraft:diamond_sword[enchantments={sharpness:5}]",
               "minecraft:iron_pickaxe[damage=100]", "minecraft:golden_apple", "minecraft:apple"]


def linha(x0, z0, cor, itens=ITENS):
    """Um baú, três transportadores em x, a curva para o sul, mais dois, a curva para o leste e outros dois, e o baú de chegada."""
    caminho = [(x0 + 1, z0), (x0 + 2, z0), (x0 + 3, z0), (x0 + 3, z0 + 1), (x0 + 3, z0 + 2), (x0 + 4, z0 + 2), (x0 + 5, z0 + 2)]
    cmds = [f"setblock {x0} {Y} {z0} minecraft:chest"]
    # A espada, a picareta e o livro encantado empilham a um: com 16 o `item replace` falha ("can only stack up to 1") e o baú ficava sem eles, nos dois jogos.
    cmds += [f"setblock {x} {Y} {z} mekanism:basic_logistical_transporter" for x, z in caminho]
    cmds.append(f"setblock {x0 + 6} {Y} {z0 + 2} minecraft:chest")
    cmds += [f"item replace block {x0} {Y} {z0} container.{i} with {item} {1 if any(n in item for n in ('sword', 'pickaxe', 'enchanted_book')) else 16}" for i, item in enumerate(itens)]
    cmds.append("#esperar 20")
    if cor is not None:
        cmds += [f"#cor {x} {Y} {z} {cor}" for x, z in caminho]
    cmds.append("#esperar 10")
    return cmds, caminho[0]


# O vidro das 18 `EnumColor`: três transportadores por tiro (a 9 blocos de altura (o `tp` em y -56,5, sem gravidade), e o `tp` fica no do meio).
VIDRO_Z = 70
VIDRO_X0 = 52
POR_TIRO = 3
# Nada a limpar entre os tiros (o `clear` do roteiro apaga o que o tiro anterior montou: um bloco de ar longe da cena).
SEM_LIMPEZA = "fill 52 -60 140 52 -60 140 minecraft:air"
# A fila de passos: depois de `PASSOS_BASE` ticks com o servidor parado, um tiro a cada `tick step 1`. Os dois jogos não começam a puxar no mesmo tick, e o tiro de cada passo deixa achar,
# a posteriori, o par (passo do original, passo do port) em que as pilhas estão no mesmo lugar.
PASSOS_BASE = 8
TIROS_DE_PASSO = 12


def tiros_de_passo(prefixo, tp, quantos=TIROS_DE_PASSO):
    return [{"name": f"{prefixo}_{k:02d}", "cmds": ["tick step 1", "#esperar 8"], "tp": tp} for k in range(quantos)]


def com_lado(tiros, prefixo, tp_lado):
    """Depois de cada tiro de passo, um tiro da mesma pose parada (sem comando: o servidor segue em `tick freeze`) com a câmera de lado: de cima a caixa tingida esconde o item, e de lado se vê
    o brilho do encantamento e o nome na pilha."""
    saida = []
    for k, tiro in enumerate(tiros):
        saida += [tiro, {"name": f"{prefixo}_lado_{k:02d}", "cmds": [], "tp": tp_lado}]
    return saida


# A fila de pilhas parada tick a tick (frente `transportador-sobras`, terceira rodada): a linha de itens com componentes (a espada encantada sai primeiro, depois a picareta gasta, o graveto com
# nome, o livro encantado), o servidor parado, e um tiro de cima e um de lado, de perto, a cada `tick step 1`, com o `progress` de cada pilha no log dos dois jogos (`#transito`, a sonda `MBI`).
PASSOS_DA_FILA = 24
TRANSITO = ["#transito 61 -60 82", "#transito 62 -60 82", "#transito 63 -60 82"]
# De cima, sobre o meio dos três primeiros transportadores, e de lado, ao norte da fileira e na altura das pilhas (z 82, a 2,5 blocos).
TP_CIMA = "62.5 -58.2 82.5 0 90"
TP_LADO = "62.5 -61.2 80.0 0 0"
# De perto, a 1,6 bloco da fileira: o brilho do encantamento e o item ocupam uns 120 pixels.
TP_PERTO = "61.9 -61.1 80.9 0 0"


# O `anyItem` do original lê o baú de baixo para cima (`TransitRequest.definedItem`, `for i = size - 1`): a espada, no último slot, sai primeiro, depois o graveto com nome, o livro, a picareta.
PILHAS_ITENS = ["minecraft:apple", "minecraft:golden_apple", "minecraft:iron_pickaxe[damage=100]", "minecraft:enchanted_book", "minecraft:stick[custom_name='Nome']", "minecraft:diamond_sword[enchantments={sharpness:5}]"]


def pilhas_json():
    cmds, primeiro = linha(60, Z, None, PILHAS_ITENS)
    base = {"name": "fila_base", "cmds": ["gamemode creative @a"] + cmds + ["tick freeze", f"#conexao {primeiro[0]} {Y} {primeiro[1]} west=pull"] + TRANSITO + ["gamemode spectator @a"], "tp": TP_CIMA}
    shots = [base]
    for k in range(1, PASSOS_DA_FILA + 1):
        shots.append({"name": f"fila_cima_{k:02d}", "cmds": ["tick step 1"] + TRANSITO + ["#esperar 8"], "tp": TP_CIMA})
        shots.append({"name": f"fila_lado_{k:02d}", "cmds": [], "tp": TP_LADO})
        shots.append({"name": f"fila_perto_{k:02d}", "cmds": [], "tp": TP_PERTO})
    # O pedido do usuário (2026-11-15): o interior do tubo azul parecia cheio de água no vídeo, sobre grama. A linha azul (`dark_blue`) com as pilhas em movimento, sobre a pedra (z 100, o chão
    # igual nos dois jogos) e sobre a grama (z 140, fora do `fill` de pedra), de cima e de lado, três ticks por tiro.
    for nome, z0 in (("azul_pedra", 100), ("azul_grama", 140)):
        cmds_b, primeiro_b = linha(60, z0, 1)
        transito = [f"#transito {x} {Y} {z0}" for x in (61, 62, 63)]
        tp_cima, tp_lado = f"62.5 -58.2 {z0 + 0.5} 0 90", f"62.5 -61.2 {z0 - 2.0} 0 0"
        shots.append({"name": f"{nome}_base", "cmds": ["tick unfreeze", "gamemode creative @a"] + cmds_b + ["tick freeze", f"#conexao {primeiro_b[0]} {Y} {primeiro_b[1]} west=pull"] + transito + ["gamemode spectator @a"], "tp": tp_cima})
        for k in range(1, 9):
            shots.append({"name": f"{nome}_cima_{k:02d}", "cmds": ["tick step 3"] + transito + ["#esperar 8"], "tp": tp_cima})
            shots.append({"name": f"{nome}_lado_{k:02d}", "cmds": [], "tp": tp_lado})
    roteiro = {"setup": SETUP, "clear": SEM_LIMPEZA, "shots": shots, "clientes": [[x, Y, Z] for x in (61, 62, 63)]}
    with open(os.path.join(HERE, "cenas/transportador_pilhas.json"), "w") as handle:
        json.dump(roteiro, handle, indent=1)
        handle.write("\n")


# A cena curta do `partialTick` (quarta rodada, decisão 246): só o tiro de perto de cada `tick step 1`, 14 passos, para repetir o mesmo estado parado muitas vezes e ler o `partialTick`
# e o `progress` do desenho (`cena-partial` e `cena-cliente` no original, `MBR` no port).
PASSOS_DO_PARTIAL = 14


def partial_json():
    cmds, primeiro = linha(60, Z, None, PILHAS_ITENS)
    shots = [{"name": "fila_base", "cmds": ["gamemode creative @a"] + cmds + ["tick freeze", f"#conexao {primeiro[0]} {Y} {primeiro[1]} west=pull"] + TRANSITO + ["gamemode spectator @a"], "tp": TP_PERTO}]
    for k in range(1, PASSOS_DO_PARTIAL + 1):
        shots.append({"name": f"fila_perto_{k:02d}", "cmds": ["tick step 1"] + TRANSITO + ["#esperar 8"], "tp": TP_PERTO})
    roteiro = {"setup": SETUP, "clear": SEM_LIMPEZA, "shots": shots, "clientes": [[x, Y, Z] for x in (61, 62, 63)]}
    with open(os.path.join(HERE, "cenas/transportador_partial.json"), "w") as handle:
        json.dump(roteiro, handle, indent=1)
        handle.write("\n")


def main():
    partial_json()
    pilhas_json()
    # Sem cor, a pilha não leva a caixa tingida e o item aparece (com a cor, a caixa de cima o esconde): as pilhas e os componentes saem na linha sem cor, e a caixa na linha pintada.
    cmds_a, primeiro_a = linha(60, Z, None)
    base_a = cmds_a + ["tick freeze", f"#conexao {primeiro_a[0]} {Y} {primeiro_a[1]} west=pull", f"tick step {PASSOS_BASE}", f"#esperar {PASSOS_BASE * 2 + 20}"]
    cmds_b, primeiro_b = linha(60, Z + 8, 4)
    base_b = ["tick unfreeze"] + cmds_b + ["tick freeze", f"#conexao {primeiro_b[0]} {Y} {primeiro_b[1]} west=pull", f"tick step {PASSOS_BASE}", f"#esperar {PASSOS_BASE * 2 + 20}"]
    cmds_c, primeiro_c = linha(80, Z, None, COMPONENTES)
    base_c = ["tick unfreeze"] + cmds_c + ["tick freeze", f"#conexao {primeiro_c[0]} {Y} {primeiro_c[1]} west=pull", f"tick step {PASSOS_BASE}", f"#esperar {PASSOS_BASE * 2 + 20}"]
    tp_a = "63.5 -56.0 83.0 0 90"
    tp_b = "63.5 -56.0 91.0 0 90"
    tp_c = "83.5 -56.0 83.0 0 90"
    tp_c_lado = "82.5 -59.6 80.2 0 0"  # de lado: ao norte da primeira fileira da linha (z 82), olhando para o sul, na altura das pilhas
    vidro_tiros = []
    n = len(range(18))
    for j in range(n // POR_TIRO):
        cmds = []
        if j == 0:
            cmds += [f"setblock {VIDRO_X0 + 2 * k} {Y} {VIDRO_Z} mekanism:basic_logistical_transporter" for k in range(n)] + ["#esperar 20"]
            cmds += [f"#cor {VIDRO_X0 + 2 * k} {Y} {VIDRO_Z} {k}" for k in range(n)] + ["#esperar 20"]
        meio = VIDRO_X0 + 2 * (POR_TIRO * j + 1) + 0.5
        vidro_tiros.append({"name": f"transportador_vidro_{j}", "cmds": cmds, "tp": f"{meio} -56.5 {VIDRO_Z + 0.5} 0 90"})
    def espectador(tiros):
        """Cada tiro começa em criativo (os cliques do Configurator, no port) e termina em espectador: o `tp` do tiro fica onde foi posto, sem gravidade nem deriva do jogador."""
        for tiro in tiros:
            tiro["cmds"] = ["gamemode creative @a"] + tiro["cmds"] + ["gamemode spectator @a"]
        return tiros

    roteiro = {
        "setup": SETUP,
        "clear": SEM_LIMPEZA,
        "shots": espectador(vidro_tiros
        + [{"name": "pilhas_base", "cmds": base_a, "tp": tp_a}] + tiros_de_passo("pilhas", tp_a)
        + [{"name": "caixas_base", "cmds": base_b, "tp": tp_b}] + tiros_de_passo("caixas", tp_b, 3)
        + [{"name": "componentes_base", "cmds": base_c, "tp": tp_c}] + com_lado(tiros_de_passo("componentes", tp_c), "componentes", tp_c_lado)),
    }
    with open(os.path.join(HERE, "cenas/transportador_visual.json"), "w") as handle:
        json.dump(roteiro, handle, indent=1)
        handle.write("\n")
    # Só a linha de componentes, para repetir a pose de lado sem os 171 cliques do vidro.
    so_componentes = dict(roteiro, shots=[tiro for tiro in roteiro["shots"] if tiro["name"].startswith("componentes")])
    with open(os.path.join(HERE, "cenas/transportador_componentes.json"), "w") as handle:
        json.dump(so_componentes, handle, indent=1)
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
