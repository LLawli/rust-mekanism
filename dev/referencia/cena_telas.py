#!/usr/bin/env python3
"""Os estados de tela que o original e o port mostram lado a lado, e o roteiro do port. A tabela está aqui. Duas saídas:

    cena_telas.py oficial ARQUIVO ESCALA... [--so NOME,NOME] escreve o arquivo `cena-telas` que `testes/CenaTelas.java` lê (um por escala)
    cena_telas.py port DESTINO ESCALA... [--so NOME,NOME]
                                      monta cada estado no Horizon pelo `horizon-ctl` e grava `port_<caso>_e<escala>.png` em DESTINO

O port roda no container `horizon` do desktop (`docker exec horizon`), com o mundo superplano aberto e o servidor parado (`tick
freeze`) enquanto o estado é montado, como o `CenaTelas`. Os nomes dos casos são os mesmos nos dois lados.
"""
import json
import os
import shlex
import subprocess
import sys

# A origem de cada tela e onde pôr o cursor, em pixels de GUI relativos ao canto da tela (um ponto dentro do elemento).
HOVER = {
    "electric": {
        "slot_entrada": (72, 25),
        "slot_saida": (124, 43),
        "slot_energia": (72, 61),
        "barra_energia": (167, 40),
        "progresso": (98, 42),
    },
    "infuser": {
        "slot_quimico": (25, 43),
        "slot_entrada": (59, 51),
        "slot_saida": (117, 51),
        "slot_energia": (151, 43),
        "barra_energia": (167, 40),
        "barra_quimico": (10, 40),
        "dump": (26, 64),
        "progresso": (88, 51),
    },
    "generator": {
        "slot_combustivel": (25, 43),
        "slot_energia": (151, 43),
        "medidor_lava": (88, 43),
        "barra_energia": (167, 40),
    },
    # `GuiEnergyCube`: o medidor largo (`GuiEnergyGauge`, `GaugeType.WIDE`, 66 por 50 em (55, 18)) e os slots de descarga (17, 35) e de carga (143, 35).
    "cubo": {
        "medidor": (88, 43),
        "slot_descarga": (25, 43),
        "slot_carga": (151, 43),
    },
}

# Por máquina: o bloco, a tela (a tabela do comparador), a capacidade de energia de uma máquina colocada por um jogador, que
# `applyImplicitComponents` recalcula (`MachineEnergyContainer.updateMaxEnergy`: consumo por tick vezes `ticksRequired`, 200),
# a entrada que tem receita, e o slot de entrada e de saída na ordem do menu.
MAQUINAS = {
    "chamber": dict(block="mekanism:enrichment_chamber", tela="electric", cap=1000, entrada="coal", slots=(0, 1), upgrade=(3, 4)),
    "crusher": dict(block="mekanism:crusher", tela="electric", cap=1600, entrada="blaze_rod", slots=(0, 1), upgrade=(3, 4)),
    "smelter": dict(block="mekanism:energized_smelter", tela="electric", cap=1000, entrada="raw_iron", slots=(0, 1), upgrade=(3, 4)),
    "infuser": dict(block="mekanism:metallurgic_infuser", tela="infuser", cap=1000, entrada="clay_ball", slots=(1, 2), upgrade=(4, 5)),
    "generator": dict(block="mekanismgenerators:heat_generator", tela="generator", cap=240, entrada=None, slots=(0, 1), upgrade=(2, 3)),
    # Os cubos de energia (`EnergyCubeTier`: 4.000.000 no básico e no ultimate 256.000.000, `Long.MAX_VALUE` no criativo): sem upgrades nem receita.
    "cubo": dict(block="mekanism:basic_energy_cube", tela="cubo", cap=4_000_000, entrada=None, slots=(0, 1), upgrade=(0, 0)),
    "cubo_ultimate": dict(block="mekanism:ultimate_energy_cube", tela="cubo", cap=256_000_000, entrada=None, slots=(0, 1), upgrade=(0, 0)),
    "cubo_criativo": dict(block="mekanism:creative_energy_cube", tela="cubo", cap=9223372036854775807, entrada=None, slots=(0, 1), upgrade=(0, 0)),
    # Os tanques (frente `tanques`, 5.ª rodada): `cap` é a capacidade do tanque em mB (`FluidTankTier` e `ChemicalTankTier`), sem energia, upgrades nem receita.
    "tanque": dict(block="mekanism:basic_fluid_tank", tela="tanque", cap=32_000, entrada=None, slots=(0, 1), upgrade=(0, 0)),
    "tanque_criativo": dict(block="mekanism:creative_fluid_tank", tela="tanque", cap=9223372036854775807, entrada=None, slots=(0, 1), upgrade=(0, 0)),
    "tanque_quimico": dict(block="mekanism:basic_chemical_tank", tela="tanque_quimico", cap=64_000, entrada=None, slots=(0, 1), upgrade=(0, 0)),
    "tanque_quimico_criativo": dict(block="mekanism:creative_chemical_tank", tela="tanque_quimico", cap=9223372036854775807, entrada=None, slots=(0, 1), upgrade=(0, 0)),
}

# O que sai do slot de saída "cheio" e o item que não é o da receita.
SAIDA_CHEIA = {"chamber": "clay_ball", "crusher": "gravel", "smelter": "glass", "infuser": "dirt"}


# Os índices do slot que enche (`ranhura`) e do de carga (`carga`) em cada tela: o cubo tem o de descarga no 0 e o de carga no 1; o gerador, a energia no 1; as máquinas, no 2 (o infuser, no 3).
SLOT_DE_ENERGIA = {"electric": (2, None), "infuser": (3, None), "generator": (1, None), "cubo": (0, 1), "tanque": (None, None), "tanque_quimico": (None, None)}


def caso(identificador, maquina, /, **args):
    return dict(nome=identificador, maquina=maquina, args=args)


def casos():
    out = []
    for m in ("chamber", "crusher", "smelter"):
        spec = MAQUINAS[m]
        entrada = f"{spec['entrada']}:2"
        out += [
            caso(f"{m}_vazia", m, energia="0"),
            caso(f"{m}_operacao", m, energia="cap", entrada=entrada, passos="100"),
            caso(f"{m}_aviso_energia", m, energia="0", entrada=entrada, passos="5"),
        ]
    # A Enrichment Chamber tem o conjunto inteiro.
    out += [
        caso("chamber_cheia", "chamber", energia="cap"),
        caso("chamber_aviso_saida", "chamber", energia="cap", entrada="coal:2", saida="mekanism:enriched_carbon:64", passos="5"),
        caso("chamber_hover_slot_vazio", "chamber", energia="0", hover=HOVER["electric"]["slot_saida"]),
        caso("chamber_hover_slot_item", "chamber", energia="0", entrada="coal:2", hover=HOVER["electric"]["slot_entrada"]),
        caso("chamber_hover_slot_energia", "chamber", energia="0", hover=HOVER["electric"]["slot_energia"]),
        caso("chamber_hover_barra", "chamber", energia="500", hover=HOVER["electric"]["barra_energia"]),
        caso("chamber_hover_progresso", "chamber", energia="cap", entrada="coal:2", passos="100",
             hover=HOVER["electric"]["progresso"]),
    ]
    out += [
        caso("infuser_vazia", "infuser", energia="0"),
        caso("infuser_operacao", "infuser", energia="cap", entrada="clay_ball:2", quimico="bio:800", passos="100"),
        caso("infuser_cheia", "infuser", energia="cap", quimico="bio:1000"),
        caso("infuser_aviso_energia", "infuser", energia="0", entrada="clay_ball:2", quimico="bio:500", passos="5"),
        caso("infuser_aviso_infusao", "infuser", energia="cap", entrada="clay_ball:2", passos="5"),
        caso("infuser_tanque_bio", "infuser", energia="0", quimico="bio:500"),
        # `ciclo` prints no original, e `ciclo_port` no port: o ciclo inteiro da animação (quadros vezes `frametime`), porque o original
        # anda um passo de animação a cada dois passos do servidor e o port um a cada passo, e o print do original tem de achar o seu
        # estado entre os do port. `base`: 10 quadros de 4 ticks; `bio`: 8 de 30; `fungi`: 4 de 40.
        caso("infuser_tanque_carbono", "infuser", energia="0", quimico="carbon:500", ciclo="40", ciclo_port="40"),
        caso("infuser_tanque_bio_ciclo", "infuser", energia="0", quimico="bio:500", ciclo="40", ciclo_port="240"),
        caso("infuser_tanque_fungi_ciclo", "infuser", energia="0", quimico="fungi:500", ciclo="40", ciclo_port="160"),
        # Os ciclos de referência dos tanques que os outros casos mostram parados (`comparar_lote.fase_do_quimico`): o mesmo químico e a mesma
        # quantidade, porque a barra com o tanque pela metade não é um recorte da barra cheia (o sprite se repete a partir do topo do líquido).
        caso("infuser_fase_bio_800", "infuser", energia="0", quimico="bio:800", ciclo="40", ciclo_port="240"),
        caso("infuser_fase_bio_1000", "infuser", energia="0", quimico="bio:1000", ciclo="40", ciclo_port="240"),
        caso("infuser_hover_barra_energia", "infuser", energia="500", hover=HOVER["infuser"]["barra_energia"]),
        caso("infuser_hover_barra_quimico", "infuser", energia="0", quimico="carbon:500",
             hover=HOVER["infuser"]["barra_quimico"]),
        caso("infuser_hover_barra_quimico_vazia", "infuser", energia="0", hover=HOVER["infuser"]["barra_quimico"]),
        caso("infuser_hover_dump", "infuser", energia="0", quimico="carbon:500", hover=HOVER["infuser"]["dump"]),
        caso("infuser_hover_slot_quimico", "infuser", energia="0", hover=HOVER["infuser"]["slot_quimico"]),
    ]
    out += [
        caso("generator_vazia", "generator", energia="0"),
        # Com a energia cheia o gerador não queima a lava, e o medidor fica no nível do caso: com 120 ela baixava a cada passo, e o nível do print deixava de ser
        # o do ciclo de referência (`comparar_lote.CICLO_DA_LAVA`), então a fase da animação não se provava. O `generator_queima` cobre o gerador queimando.
        caso("generator_lava_meio", "generator", energia="cap", lava="500"),
        caso("generator_lava_cheia", "generator", energia="cap", lava="1000"),
        # Com a energia cheia o gerador não queima a lava: com 120 ela baixava a cada passo e o medidor mudava de nível em vez de animar.
        # O `lava_still` tem 38 quadros de 2 ticks, sem interpolação.
        caso("generator_lava_ciclo", "generator", energia="cap", lava="500", ciclo="40", ciclo_port="76"),
        # O medidor cheio, parado: o ciclo de referência dos casos com a lava a 1000 (`comparar_lote.CICLO_DA_LAVA`).
        caso("generator_fase_lava_1000", "generator", energia="cap", lava="1000", ciclo="40", ciclo_port="76"),
        # O medidor com o cursor em cima (o realce e o tooltip), parado: o ciclo de referência de `generator_hover_medidor`.
        caso("generator_fase_lava_hover", "generator", energia="cap", lava="500", hover=HOVER["generator"]["medidor_lava"], ciclo="40", ciclo_port="76"),
        caso("generator_hover_barra", "generator", energia="cap", lava="500", hover=HOVER["generator"]["barra_energia"]),
        caso("generator_hover_medidor", "generator", energia="cap", lava="500", hover=HOVER["generator"]["medidor_lava"]),
        caso("generator_hover_medidor_vazio", "generator", energia="0", hover=HOVER["generator"]["medidor_lava"]),
        caso("generator_hover_slot", "generator", energia="0", hover=HOVER["generator"]["slot_combustivel"]),
        # Queima a lava com a energia subindo: a barra e o `MEKTRACE` do port, contra o servidor do original, passo a passo.
        caso("generator_queima", "generator", energia="0", lava="500", passos="100"),
    ]
    return out


# A janela de upgrades (0,0 = canto da tela). A janela abre em (-11, 15): a aba é clicada em (190, 19), uma linha da lista em (29, 40), o
# botão Uninstall em (132, 75) e o rótulo "Supported:" em (-2, 85).
ABA = (190, 19)
LINHA = (29, 40)
UNINSTALL = (132, 75)


def casos_aba():
    out = []
    todos = "speed:3,energy:2,muffling:1,xp_containment:1"
    for m in ("chamber", "crusher", "smelter", "infuser"):
        out += [
            caso(f"{m}_aba_fechada", m, energia="0"),
            caso(f"{m}_aba_vazia", m, energia="0", clicar=f"{ABA[0]},{ABA[1]}"),
            caso(f"{m}_aba_upgrades", m, energia="0", upgrades=todos, clicar=f"{ABA[0]},{ABA[1]}"),
        ]
    out += [
        caso("chamber_aba_hover", "chamber", energia="0", hover=ABA),
        caso("chamber_aba_selecionado", "chamber", energia="0", upgrades=todos,
             clicar=f"{ABA[0]},{ABA[1]};{LINHA[0]},{LINHA[1]}"),
        caso("chamber_aba_botao_hover", "chamber", energia="0", upgrades=todos,
             clicar=f"{ABA[0]},{ABA[1]};{LINHA[0]},{LINHA[1]}", hover=UNINSTALL),
        caso("chamber_aba_instalando", "chamber", energia="0", upgradeentrada="speed:5", passos="10",
             clicar=f"{ABA[0]},{ABA[1]}"),
        caso("chamber_aba_instalado", "chamber", energia="0", upgradeentrada="speed:5", passos="30",
             clicar=f"{ABA[0]},{ABA[1]}"),
        caso("chamber_aba_saida", "chamber", energia="0", upgrades="speed:3", upgradesaida="speed:7",
             clicar=f"{ABA[0]},{ABA[1]}"),
        caso("chamber_aba_hover_lista", "chamber", energia="0", upgrades=todos,
             clicar=f"{ABA[0]},{ABA[1]}", hover=LINHA),
        caso("chamber_aba_hover_aceitos", "chamber", energia="0", upgrades="speed:1",
             clicar=f"{ABA[0]},{ABA[1]}", hover=(-2, 95)),
        # A ordem da lista do original é a de um `Object2IntOpenHashMap` de chaves por identidade (muda a cada partida): com mais de um
        # tipo o print do original só se compara por linha. Com um tipo só a lista, a seleção e o texto têm ordem única.
        caso("chamber_aba_um_upgrades", "chamber", energia="0", upgrades="speed:3", clicar=f"{ABA[0]},{ABA[1]}"),
        caso("chamber_aba_um_selecionado", "chamber", energia="0", upgrades="speed:3",
             clicar=f"{ABA[0]},{ABA[1]};{LINHA[0]},{LINHA[1]}"),
        caso("chamber_aba_um_botao_hover", "chamber", energia="0", upgrades="speed:3",
             clicar=f"{ABA[0]},{ABA[1]};{LINHA[0]},{LINHA[1]}", hover=UNINSTALL),
        caso("chamber_aba_um_hover_lista", "chamber", energia="0", upgrades="speed:3",
             clicar=f"{ABA[0]},{ABA[1]}", hover=LINHA),
        # A regra do clique: com a janela aberta, o Dump (26, 64) fica atrás dela e o clique é consumido (o químico fica). Com a janela
        # fechada pelo X (-1, 25), o mesmo clique chega ao Dump e esvazia o tanque.
        caso("infuser_aba_clique_dentro", "infuser", energia="0", quimico="bio:500",
             clicar=f"{ABA[0]},{ABA[1]};26,64"),
        caso("infuser_aba_clique_fechada", "infuser", energia="0", quimico="bio:500",
             clicar=f"{ABA[0]},{ABA[1]};-1,25;26,64"),
    ]
    return out


# A aba de xp (`GuiXpContainmentTab`: 26 por 26 em `imageWidth`, 90; o botão de dentro, 18 por 18 em (179, 94)). Aparece com o upgrade
# `xp_containment` instalado. Os pontos de cursor: dentro do botão, na borda da aba (fora do botão), e dentro do botão mas atrás da janela.
XP_BOTAO = (188, 103)
XP_BORDA = (200, 115)
XP_ATRAS_DA_JANELA = (183, 100)
XP_FORA_DA_JANELA = (192, 110)
XP = "xp_containment:1"
NOME_COR = '{"text":"Camara","color":"red","bold":true}'
NOME_FILHO = '{"text":"Camara","color":"red","bold":true,"extra":[{"text":"Azul","color":"#3355FF","italic":true}]}'


def casos_xp():
    out = []
    for m in ("chamber", "crusher", "smelter", "infuser"):
        out += [
            caso(f"{m}_xp_sem", m, energia="0"),
            caso(f"{m}_xp_com", m, energia="0", upgrades=XP),
        ]
    out += [
        caso("chamber_xp_hover", "chamber", energia="0", upgrades=XP, hover=XP_BOTAO),
        caso("chamber_xp_hover_borda", "chamber", energia="0", upgrades=XP, hover=XP_BORDA),
        caso("infuser_xp_hover", "infuser", energia="0", upgrades=XP, hover=XP_BOTAO),
        # Sem o upgrade a aba não existe: o cursor no mesmo lugar não mostra nada.
        caso("chamber_xp_hover_sem", "chamber", energia="0", hover=XP_BOTAO),
        # A janela de upgrades aberta cobre parte da aba: atrás dela o botão não tem foco nem tooltip (`isMouseOverCheckWindows`).
        caso("chamber_xp_janela", "chamber", energia="0", upgrades=XP, clicar=f"{ABA[0]},{ABA[1]}"),
        caso("chamber_xp_janela_atras", "chamber", energia="0", upgrades=XP, clicar=f"{ABA[0]},{ABA[1]}", hover=XP_ATRAS_DA_JANELA),
        caso("chamber_xp_janela_fora", "chamber", energia="0", upgrades=XP, clicar=f"{ABA[0]},{ABA[1]}", hover=XP_FORA_DA_JANELA),
        # O upgrade instalando (20 ticks por upgrade): antes de acabar a aba não aparece, depois aparece.
        caso("chamber_xp_instalando", "chamber", energia="0", upgradeentrada=XP, passos="10"),
        caso("chamber_xp_instalado", "chamber", energia="0", upgradeentrada=XP, passos="30"),
        # Um clique na aba (sem xp guardado nada acontece) deixa a tela como estava.
        caso("chamber_xp_clique", "chamber", energia="0", upgrades=XP, clicar=f"{XP_BOTAO[0]},{XP_BOTAO[1]}"),
        # O título com o componente inteiro do nome: cor, negrito, e um filho de outra cor em itálico.
        caso("chamber_nome_cor", "chamber", energia="0", nome=NOME_COR),
        caso("chamber_nome_filho", "chamber", energia="0", nome=NOME_FILHO),
        caso("infuser_nome_cor", "infuser", energia="0", nome=NOME_COR),
        # O Heat Generator também aceita nome (`nameable = hasGui() && !AttributeGui.hasCustomName()`).
        caso("generator_nome_cor", "generator", energia="0", nome=NOME_COR),
        caso("generator_nome_filho", "generator", energia="0", nome=NOME_FILHO),
    ]
    return out


# A aba de energia (`GuiEnergyTab`, -26 por 137, 26 por 26): o cursor dentro dela. Os casos `_aba_energia` têm a aba parada, com o cursor nela, e
# nos estados que mudam o texto: máquina ativa, upgrades de velocidade (o consumo por tick), energia cheia, o gerador produzindo.
ENERGIA_CURSOR = (-13, 150)


def casos_energia():
    out = []
    for m in ("chamber", "crusher", "smelter", "infuser"):
        spec = MAQUINAS[m]
        entrada = f"{spec['entrada']}:2"
        extra = dict(quimico="bio:800") if m == "infuser" else {}
        out += [
            caso(f"{m}_aba_energia", m, energia="0"),
            caso(f"{m}_aba_energia_hover", m, energia="0", hover=ENERGIA_CURSOR),
            caso(f"{m}_aba_energia_hover_cheia", m, energia="cap", hover=ENERGIA_CURSOR),
            caso(f"{m}_aba_energia_hover_ativa", m, energia="cap", entrada=entrada, passos="10", hover=ENERGIA_CURSOR, **extra),
            # A capacidade muda com os upgrades (`cap` valeria a de uma máquina sem eles): um valor que cabe em qualquer uma.
            caso(f"{m}_aba_energia_hover_velocidade", m, energia="300", entrada=entrada, passos="10", upgrades="speed:3,energy:2",
                 hover=ENERGIA_CURSOR, **extra),
        ]
    # A máquina operando por 10 passos, sem a aba: o controle da barra de químico do Infuser nos casos `ativa`.
    out.append(caso("infuser_operacao_10", "infuser", energia="cap", entrada="clay_ball:2", quimico="bio:800", passos="10"))
    out += [
        caso("generator_aba_energia", "generator", energia="0"),
        caso("generator_aba_energia_hover", "generator", energia="0", hover=ENERGIA_CURSOR),
        caso("generator_aba_energia_hover_produzindo", "generator", energia="0", lava="500", passos="100", hover=ENERGIA_CURSOR),
    ]
    return out


# A aba de calor do Heat Generator (`GuiHeatTab`, -26 por 109, 26 por 26) e a de upgrades dele (a mesma do `ABA`, à direita). O clique troca a unidade de
# temperatura do cliente (K, C, R, F, STP, e volta ao K na quinta), que fica guardada entre os casos: `desfazer` completa a volta no fim do caso.
CALOR = (-13, 122)


def casos_gerador():
    todos = "muffling:1"
    cliques = lambda n: ";".join([f"{CALOR[0]},{CALOR[1]}"] * n)
    out = [
        caso("generator_aba_calor_hover", "generator", energia="0", hover=CALOR),
        caso("generator_aba_calor_hover_produzindo", "generator", energia="0", lava="500", passos="100", hover=CALOR),
    ]
    for nome, n in (("c", 1), ("r", 2), ("f", 3), ("stp", 4)):
        out.append(caso(f"generator_aba_calor_unidade_{nome}", "generator", energia="0", lava="500", passos="100", clicar=cliques(n),
                        desfazer=cliques(5 - n), hover=CALOR))
    out += [
        caso("generator_aba_fechada", "generator", energia="0"),
        caso("generator_aba_vazia", "generator", energia="0", clicar=f"{ABA[0]},{ABA[1]}"),
        caso("generator_aba_upgrades", "generator", energia="0", upgrades=todos, clicar=f"{ABA[0]},{ABA[1]}"),
        caso("generator_aba_selecionado", "generator", energia="0", upgrades=todos, clicar=f"{ABA[0]},{ABA[1]};{LINHA[0]},{LINHA[1]}"),
        caso("generator_aba_botao_hover", "generator", energia="0", upgrades=todos, clicar=f"{ABA[0]},{ABA[1]};{LINHA[0]},{LINHA[1]}", hover=UNINSTALL),
        caso("generator_aba_instalando", "generator", energia="0", upgradeentrada=todos, passos="10", clicar=f"{ABA[0]},{ABA[1]}"),
        caso("generator_aba_instalado", "generator", energia="0", upgradeentrada=todos, passos="30", clicar=f"{ABA[0]},{ABA[1]}"),
        caso("generator_aba_hover_aceitos", "generator", energia="0", clicar=f"{ABA[0]},{ABA[1]}", hover=(-2, 95)),
    ]
    return out


# A aba de segurança (`GuiSecurityTab`, 26 por 26 em (176, 34); o botão de 18 por 18 em (179, 38)): o cursor e o clique no centro do botão. O clique do dono avança o modo
# (`PUBLIC`, `PRIVATE`, `TRUSTED`), e o tooltip diz o modo e o dono.
ABA_SEGURANCA = (188, 47)
# No cubo de energia a aba fica em y = 6 (`GuiEnergyCube.addSecurityTab`): o botão em (179, 10).
ABA_SEGURANCA_CUBO = (188, 19)


def casos_seguranca():
    out = []
    for m in ("chamber", "infuser", "generator", "cubo"):
        aba = ABA_SEGURANCA_CUBO if m == "cubo" else ABA_SEGURANCA
        clique = f"{aba[0]},{aba[1]}"
        extra = {}
        out += [
            caso(f"{m}_seg_publica", m, energia="0", **extra),
            caso(f"{m}_seg_hover", m, energia="0", hover=aba, **extra),
            caso(f"{m}_seg_privada", m, energia="0", clicar=clique, **extra),
            caso(f"{m}_seg_privada_hover", m, energia="0", clicar=clique, hover=aba, **extra),
            caso(f"{m}_seg_confiavel_hover", m, energia="0", clicar=f"{clique};{clique}", hover=aba, **extra),
        ]
    return out


# A aba de redstone (`GuiRedstoneControlTab`, 26 por 26 em (176, 137); o botão de 18 por 18 em (179, 141)): o cursor e o clique no centro do botão. O clique esquerdo avança o modo
# (`DISABLED`, `HIGH`, `LOW`); sem sinal, `HIGH` mostra o aviso (a aba desenhada com o padrão de `warning` e a aba de avisos da esquerda).
ABA_REDSTONE = (188, 150)


def casos_redstone():
    out = []
    for m in ("chamber", "infuser", "generator", "cubo"):
        extra = dict(lava="500") if m == "generator" else {}
        clique = f"{ABA_REDSTONE[0]},{ABA_REDSTONE[1]}"
        out += [
            caso(f"{m}_red_desligada", m, energia="0", **extra),
            caso(f"{m}_red_desligada_hover", m, energia="0", hover=ABA_REDSTONE, **extra),
            caso(f"{m}_red_alta", m, energia="0", clicar=clique, **extra),
            caso(f"{m}_red_alta_hover", m, energia="0", clicar=clique, hover=ABA_REDSTONE, **extra),
            caso(f"{m}_red_baixa", m, energia="0", clicar=f"{clique};{clique}", **extra),
            caso(f"{m}_red_baixa_hover", m, energia="0", clicar=f"{clique};{clique}", hover=ABA_REDSTONE, **extra),
        ]
    return out


# A aba do livro de receitas (`GuiRecipeBookWindowTab`, 26 por 26 em (176, 62); o botão de 18 por 18 em (179, 66)): só as quatro máquinas (`IRecipeLookupHandler`) a têm.
# O cursor no centro do botão dá o fundo em foco e o tooltip `Recipe Book`; o clique abre a janela do livro no original, que o port ainda não tem (decisão 260), então não há caso de clique.
LIVRO_BOTAO = (188, 75)
LIVRO_BORDA = (198, 75)
# Com a janela de upgrades aberta (`ABA`), atrás dela o botão não tem foco nem tooltip (`isMouseOverCheckWindows`); fora dela tem.
LIVRO_ATRAS_DA_JANELA = (183, 75)
LIVRO_FORA_DA_JANELA = (192, 75)
# Dentro do livro aberto: a janela fica onde a última posição guardada a deixou (`SelectedWindowData.lastPosition`, `calculateTarget`), nos dois
# jogos em (0, 88) da tela em guiScale 2 e (0, 87) em guiScale 3 (encostada embaixo), e o livro 10 abaixo (`Y_OFFSET`). Os pontos caem dentro
# do elemento nas duas: a caixa de busca (25, 13, 81 por 14), o filtro (110, 12, 26 por 16) e a seta da página seguinte (93, 137, 12 por 17).
LIVRO_BUSCA = (60, 117)
LIVRO_FILTRO = (120, 117)
LIVRO_SETA = (98, 243)
# A seta de voltar (55 à esquerda da de avançar: `backButton` em `x + 38` e `forwardButton` em `x + 93`) e a primeira aba de categoria (35 por
# 27, 30 à esquerda do livro, 27 abaixo da bússola).
LIVRO_SETA_VOLTA = (43, 243)
LIVRO_ABA = (-15, 140)


# O primeiro botão da grade do livro (25 por 25 em (11, 31) do livro) e os dois primeiros botões do overlay das variantes que o direito
# abre nele (24 por 24 em (15, 36) e (40, 36) do livro): os pontos caem dentro deles em guiScale 2 e 3.
LIVRO_GRADE = (23, 141)
LIVRO_OVERLAY_0 = (27, 146)
LIVRO_OVERLAY_1 = (52, 146)
# O botão de fechar da janela do livro (8 por 8 em (6, 6) da janela).
LIVRO_FECHAR = (10, 98)
# Um ponto do fundo do livro, sem botão (a última fila da grade, vazia depois de uma busca).
LIVRO_FUNDO = (100, 215)


def casos_livro_colocar():
    abre = f"{LIVRO_BOTAO[0]},{LIVRO_BOTAO[1]}"
    busca = f"{LIVRO_BUSCA[0]},{LIVRO_BUSCA[1]}"
    grade = f"{LIVRO_GRADE[0]},{LIVRO_GRADE[1]}"

    def livro(nome, maquina, texto, depois, **args):
        return caso(nome, maquina, energia="0", receitas="1", capturas="6", capturas_port="6", clicar=";".join([abre, busca, f"t={texto}", *depois]), **args)

    return [
        # O clique esquerdo coloca 8 de serragem (a receita pede 8), e com o shift tudo o que há.
        livro("chamber_livro_colocar", "chamber", "charcoal_dust", [grade], itens="9:mekanism:sawdust:20"),
        livro("chamber_livro_colocar_shift", "chamber", "charcoal_dust", [grade + ",s"], itens="9:mekanism:sawdust:20"),
        # Sem o ingrediente: o fantasma nos slots de entrada e de saída.
        livro("chamber_livro_fantasma", "chamber", "charcoal_dust", [grade]),
        # A janela fechada no X e aberta de novo na mesma tela: a busca continua na caixa (`initVisuals` a refaz com o texto antigo).
        caso("chamber_livro_reabrir", "chamber", energia="0", receitas="1", capturas="6", capturas_port="6",
             clicar=";".join([abre, busca, "t=iron", f"{LIVRO_FECHAR[0]},{LIVRO_FECHAR[1]}", abre])),
        # O clique no slot de entrada (um slot de receita) tira os fantasmas (`slotClicked`).
        livro("chamber_livro_fantasma_slot", "chamber", "charcoal_dust", [grade, "%d,%d" % HOVER["electric"]["slot_entrada"]]),
        # O fantasma, depois o ingrediente chega e o Enter repete a última receita. A caixa de busca continua focada depois do clique na
        # receita e prenderia o Enter (`capturesInput`): um clique no fundo do livro (`LIVRO_FUNDO`) tira o foco dela.
        livro("chamber_livro_enter", "chamber", "charcoal_dust", [grade, f"{LIVRO_FUNDO[0]},{LIVRO_FUNDO[1]}", "i=9:mekanism:sawdust:20", "k=enter"]),
        # Com a caixa focada o Enter não é do livro: o fantasma fica.
        livro("chamber_livro_enter_focado", "chamber", "charcoal_dust", [grade, "i=9:mekanism:sawdust:20", "k=enter"]),
        # O direito num botão de várias receitas abre o overlay (uma fazível, com 5 de ferro bruto, e as outras não).
        livro("chamber_livro_overlay", "chamber", "iron_dust", [grade + ",r"], itens="9:raw_iron:5"),
        livro("chamber_livro_overlay_colocar", "chamber", "iron_dust", [grade + ",r", f"{LIVRO_OVERLAY_0[0]},{LIVRO_OVERLAY_0[1]}"], itens="9:raw_iron:5"),
        livro("chamber_livro_overlay_fantasma", "chamber", "iron_dust", [grade + ",r", f"{LIVRO_OVERLAY_1[0]},{LIVRO_OVERLAY_1[1]}"], itens="9:raw_iron:5"),
        # O filtro com itens no inventário (o que a 5.ª rodada mediu só no port).
        caso("chamber_livro_filtro_itens", "chamber", energia="0", receitas="1", capturas="6", capturas_port="6", itens="9:iron_ore:3,10:redstone:5,11:cobblestone:2",
             clicar=f"{abre};{LIVRO_FILTRO[0]},{LIVRO_FILTRO[1]}", desfazer=f"{LIVRO_FILTRO[0]},{LIVRO_FILTRO[1]}"),
        # As outras máquinas: o livro de cada uma, a colocação e o fantasma.
        caso("crusher_livro_receitas", "crusher", energia="0", receitas="1", capturas="8", capturas_port="8", clicar=abre),
        livro("crusher_livro_colocar", "crusher", "gravel", [grade], itens="9:cobblestone:10"),
        livro("crusher_livro_fantasma", "crusher", "gravel", [grade]),
        caso("smelter_livro_receitas", "smelter", energia="0", receitas="1", capturas="8", capturas_port="8", clicar=abre),
        caso("infuser_livro_receitas", "infuser", energia="0", receitas="1", capturas="8", capturas_port="8", clicar=abre),
        livro("infuser_livro_colocar", "infuser", "infused_alloy", [grade], itens="9:copper_ingot:5", quimico="redstone:10"),
        # O tanque vazio: o fantasma no slot, na barra de químico e na saída.
        livro("infuser_livro_fantasma", "infuser", "infused_alloy", [grade], itens="9:copper_ingot:5"),
        # O Energized Smelter: a colocação lança `ClassCastException` no servidor do original, que a registra e segue (decisão 268): nada
        # muda nos dois jogos.
        livro("smelter_livro_clique", "smelter", "baked_potato", [grade], itens="9:potato:5"),
        # A 7.ª rodada: a segunda página do livro das outras máquinas (a ordem das coleções, decisão 269; a da Chamber é `chamber_livro_pagina`).
        *[caso(f"{m}_livro_pagina", m, energia="0", receitas="1", capturas="8", capturas_port="8", clicar=f"{abre};{LIVRO_SETA[0]},{LIVRO_SETA[1]}")
          for m in ("crusher", "smelter", "infuser")],
    ]


def casos_livro_dicas():
    """A 7.ª rodada: os tooltips do livro (a receita, com `Right Click for More` quando há várias; o filtro; as setas; a aba, que não tem; o
    slot com fantasma) e a caixa de busca com cursor e seleção (`EditBox`)."""
    abre = f"{LIVRO_BOTAO[0]},{LIVRO_BOTAO[1]}"
    busca = f"{LIVRO_BUSCA[0]},{LIVRO_BUSCA[1]}"
    grade = f"{LIVRO_GRADE[0]},{LIVRO_GRADE[1]}"
    fundo = f"{LIVRO_FUNDO[0]},{LIVRO_FUNDO[1]}"
    filtro = f"{LIVRO_FILTRO[0]},{LIVRO_FILTRO[1]}"
    seta = f"{LIVRO_SETA[0]},{LIVRO_SETA[1]}"

    def livro(nome, passos, **args):
        return caso(nome, "chamber", energia="0", receitas="1", capturas="6", capturas_port="6", clicar=";".join([abre, *passos]), **args)

    return [
        livro("chamber_livro_dica_receita", [busca, "t=charcoal_dust", fundo], hover=LIVRO_GRADE),
        livro("chamber_livro_dica_varias", [busca, "t=iron_dust", fundo], hover=LIVRO_GRADE),
        livro("chamber_livro_dica_filtro", [], hover=LIVRO_FILTRO),
        livro("chamber_livro_dica_filtro_ligado", [filtro], hover=LIVRO_FILTRO, desfazer=filtro),
        livro("chamber_livro_dica_seta", [], hover=LIVRO_SETA),
        livro("chamber_livro_dica_seta_volta", [seta], hover=LIVRO_SETA_VOLTA),
        livro("chamber_livro_dica_aba", [], hover=LIVRO_ABA),
        livro("chamber_livro_dica_fantasma", [busca, "t=charcoal_dust", grade, fundo], hover=HOVER["electric"]["slot_entrada"]),
        # A caixa de busca: o cursor no meio do texto (a barra de 1 pixel), a seleção (o realce invertido), a palavra apagada com o Ctrl,
        # o texto que rola com o cursor, a seleção por arrasto e o texto trocado por cima de tudo selecionado.
        livro("chamber_livro_busca_cursor", [busca, "t=iron_dust", "k=left", "k=left", "k=left"]),
        livro("chamber_livro_busca_selecao", [busca, "t=iron_dust", "k=left", "k=s-left", "k=s-left", "k=s-left"]),
        # (O `horizon-ctl key` não conhece Home, End nem Delete: os casos usam as setas, o Ctrl, o shift e o Backspace.)
        livro("chamber_livro_busca_inicio", [busca, "t=iron_dust", "k=cs-left", "k=cs-left"]),
        livro("chamber_livro_busca_palavra", [busca, "t=iron_and_gold", "k=c-backspace", "k=c-left"]),
        livro("chamber_livro_busca_rolagem", [busca, "t=block_of_raw_iron_and_more", *["k=c-left"] * 6, "k=right", "k=right"]),
        livro("chamber_livro_busca_rolagem_fim", [busca, "t=block_of_raw_iron_and_more", *["k=c-left"] * 6, *["k=c-right"] * 6, "k=s-left", "k=s-left"]),
        livro("chamber_livro_busca_arrasto", [busca, "t=iron_dust", f"d={LIVRO_BUSCA[0] + 20},{LIVRO_BUSCA[1]},{LIVRO_BUSCA[0] - 28},{LIVRO_BUSCA[1]}"]),
        livro("chamber_livro_busca_tudo", [busca, "t=iron_dust", "k=c-a", "t=gold", "k=left", "k=backspace"]),
    ]


def casos_livro():
    out = []
    for m in ("chamber", "crusher", "smelter", "infuser"):
        out += [
            caso(f"{m}_livro", m, energia="0"),
            caso(f"{m}_livro_hover", m, energia="0", hover=LIVRO_BOTAO),
        ]
    out += [
        # O suporte em volta do botão não responde: sem foco e sem tooltip.
        caso("chamber_livro_hover_borda", "chamber", energia="0", hover=LIVRO_BORDA),
        # A aba de xp vem 28 abaixo, com o livro na tela (`y = 90`), e as duas têm foco quando o cursor está sobre cada uma.
        caso("chamber_livro_xp", "chamber", energia="0", upgrades=XP, hover=LIVRO_BOTAO),
        caso("infuser_livro_xp", "infuser", energia="0", upgrades=XP),
        caso("chamber_livro_janela_atras", "chamber", energia="0", clicar=f"{ABA[0]},{ABA[1]}", hover=LIVRO_ATRAS_DA_JANELA),
        caso("chamber_livro_janela_fora", "chamber", energia="0", clicar=f"{ABA[0]},{ABA[1]}", hover=LIVRO_FORA_DA_JANELA),
        # O clique abre a janela do livro (`GuiRecipeBookWindow`), vazia: nenhuma receita desbloqueada.
        caso("chamber_livro_clique", "chamber", energia="0", clicar=f"{LIVRO_BOTAO[0]},{LIVRO_BOTAO[1]}"),
        # A janela do livro com todas as receitas desbloqueadas (`/recipe give @a *` nos dois jogos).
        caso("chamber_livro_receitas", "chamber", energia="0", receitas="1", capturas="8", capturas_port="8", clicar=f"{LIVRO_BOTAO[0]},{LIVRO_BOTAO[1]}"),
        # A busca: o clique na caixa e o texto `iron`.
        caso("chamber_livro_busca", "chamber", energia="0", receitas="1", capturas_port="8", clicar=f"{LIVRO_BOTAO[0]},{LIVRO_BOTAO[1]};{LIVRO_BUSCA[0]},{LIVRO_BUSCA[1]}", digitar="iron"),
        # O filtro ligado (e desligado ao fim, porque o livro o guarda por tipo).
        caso("chamber_livro_filtro", "chamber", energia="0", receitas="1", capturas_port="8", clicar=f"{LIVRO_BOTAO[0]},{LIVRO_BOTAO[1]};{LIVRO_FILTRO[0]},{LIVRO_FILTRO[1]}",
             desfazer=f"{LIVRO_FILTRO[0]},{LIVRO_FILTRO[1]}"),
        # A segunda página. Neste caso e no de cima o original também tira várias: os botões que mostram receitas destacadas crescem e voltam por
        # 15 ticks depois de aparecer (`RecipeButton.animationTime`), e o primeiro print do original cai no meio da animação.
        caso("chamber_livro_pagina", "chamber", energia="0", receitas="1", capturas="8", capturas_port="8", clicar=f"{LIVRO_BOTAO[0]},{LIVRO_BOTAO[1]};{LIVRO_SETA[0]},{LIVRO_SETA[1]}"),
        # A 6.ª rodada: a colocação pelo livro, o fantasma e o overlay das variantes (decisão 268). A busca deixa na grade só o botão da
        # receita (`LIVRO_GRADE`, o primeiro), para o clique não depender da fase do `SlotSelectTime`.
        *casos_livro_colocar(),
        *casos_livro_dicas(),
    ]
    return out


def casos_cubo():
    """O Energy Cube (frente `cubo-pixel`): vazio, parcial, cheio, o criativo, a aba de energia, as janelas de lados e de transporte e o cursor sobre o medidor.
    As abas de segurança, redstone e livro ficam como o `CenaTelas` as deixa (o cubo não tem a do livro)."""
    items = ("input", "output", "none", "input", "output", "none")
    energia = ("none", "input", "output", "input", "output", "none")
    lados_cubo = lados(items=items, energy=energia)
    transporte = dict(strict="1", cor_saida="red", cor_entrada="front:blue,left:yellow,top:dark_red")
    aba_energia_cfg = (JANELA[0] - 12, JANELA[1] + 15 + 28)
    out = [
        # O medidor é a tira animada `mek_liquid/energy` (32 quadros de 2 ticks): o ciclo de cada nível e do cursor em cima serve de referência (`comparar_lote.CICLO_DA_ENERGIA`).
        caso("cubo_ciclo_parcial", "cubo", energia="1500000", ciclo="40", ciclo_port="64"),
        caso("cubo_ciclo_cheio", "cubo", energia="cap", ciclo="40", ciclo_port="64"),
        caso("cubo_ciclo_ultimate", "cubo_ultimate", energia="100000000", ciclo="40", ciclo_port="64"),
        caso("cubo_ciclo_criativo", "cubo_criativo", energia="cap", ciclo="40", ciclo_port="64"),
        caso("cubo_ciclo_hover_parcial", "cubo", energia="1500000", hover=HOVER["cubo"]["medidor"], ciclo="40", ciclo_port="64"),
        caso("cubo_ciclo_hover_cheio", "cubo", energia="cap", hover=HOVER["cubo"]["medidor"], ciclo="40", ciclo_port="64"),
        caso("cubo_ciclo_hover_criativo", "cubo_criativo", energia="cap", hover=HOVER["cubo"]["medidor"], ciclo="40", ciclo_port="64"),
        caso("cubo_vazio", "cubo", energia="0"),
        caso("cubo_parcial", "cubo", energia="1500000"),
        caso("cubo_cheio", "cubo", energia="cap"),
        caso("cubo_ultimate_parcial", "cubo_ultimate", energia="100000000"),
        caso("cubo_criativo", "cubo_criativo", energia="0"),
        caso("cubo_criativo_cheio", "cubo_criativo", energia="cap"),
        caso("cubo_hover_medidor_vazio", "cubo", energia="0", hover=HOVER["cubo"]["medidor"]),
        caso("cubo_hover_medidor", "cubo", energia="1500000", hover=HOVER["cubo"]["medidor"]),
        caso("cubo_hover_medidor_cheio", "cubo", energia="cap", hover=HOVER["cubo"]["medidor"]),
        caso("cubo_hover_medidor_criativo", "cubo_criativo", energia="cap", hover=HOVER["cubo"]["medidor"]),
        caso("cubo_hover_slot_descarga", "cubo", energia="1500000", hover=HOVER["cubo"]["slot_descarga"]),
        caso("cubo_hover_slot_carga", "cubo", energia="1500000", hover=HOVER["cubo"]["slot_carga"]),
        caso("cubo_aba_energia", "cubo", energia="1500000"),
        caso("cubo_aba_energia_hover", "cubo", energia="1500000", hover=ENERGIA_CURSOR),
        caso("cubo_criativo_aba_energia_hover", "cubo_criativo", energia="cap", hover=ENERGIA_CURSOR),
        caso("cubo_lados", "cubo", energia="1500000", lados=lados_cubo, clicar=f"{ABA_LADOS[0]},{ABA_LADOS[1]}"),
        caso("cubo_lados_energia", "cubo", energia="1500000", lados=lados_cubo,
             clicar=f"{ABA_LADOS[0]},{ABA_LADOS[1]};{aba_energia_cfg[0]},{aba_energia_cfg[1]}"),
        caso("cubo_lados_hover_botao", "cubo", energia="1500000", lados=lados_cubo, clicar=f"{ABA_LADOS[0]},{ABA_LADOS[1]}", hover=na_janela(101, 80)),
        caso("cubo_transporte", "cubo", energia="1500000", lados=lados_cubo, clicar=f"{ABA_TRANSPORTE[0]},{ABA_TRANSPORTE[1]}", **transporte),
        caso("cubo_transporte_hover_cor", "cubo", energia="1500000", lados=lados_cubo, clicar=f"{ABA_TRANSPORTE[0]},{ABA_TRANSPORTE[1]}",
             hover=na_janela(120, 57), **transporte),
    ]
    return out


def casos_slot_energia():
    """O slot de energia funcional (frente `slot-energia`, `EnergyInventorySlot.fillContainerFromSlot` e `drainContainerIntoSlot`): o item com energia no slot, a máquina ou o cubo
    enchendo dele e o gerador esvaziando nele, com a barra de energia da tela e a do item, e o tooltip do item sob o cursor (a energia exata). `ranhura` é o slot que enche (descarga
    do cubo, energia do gerador e das máquinas) e `carga` o de carga do cubo. O tablet tem 1.000.000 e a taxa de 5.000 por tick; o Free Runners, 64.000."""
    runners = "mekanism:free_runners:30000"
    return [
        caso("chamber_slot_tablet_antes", "chamber", energia="0", ranhura="mekanism:energy_tablet:30000", passos="1"),
        caso("chamber_slot_tablet_depois", "chamber", energia="0", ranhura="mekanism:energy_tablet:30000", passos="2"),
        caso("chamber_slot_tablet_hover", "chamber", energia="0", ranhura="mekanism:energy_tablet:30000", passos="2", hover=HOVER["electric"]["slot_energia"]),
        caso("chamber_slot_equipamento_hover", "chamber", energia="0", ranhura="mekanism:atomic_disassembler:500000", passos="5", hover=HOVER["electric"]["slot_energia"]),
        caso("infuser_slot_tablet_depois", "infuser", energia="0", ranhura="mekanism:energy_tablet:30000", passos="2"),
        caso("generator_slot_antes", "generator", energia="200", lava="1000", ranhura=runners, passos="1"),
        caso("generator_slot_depois", "generator", energia="200", lava="1000", ranhura=runners, passos="60"),
        caso("generator_slot_hover_antes", "generator", energia="200", lava="1000", ranhura=runners, passos="1", hover=HOVER["generator"]["slot_energia"]),
        caso("generator_slot_hover_depois", "generator", energia="200", lava="1000", ranhura=runners, passos="60", hover=HOVER["generator"]["slot_energia"]),
        caso("generator_slot_cheio_hover", "generator", energia="200", lava="1000", ranhura="mekanism:free_runners:64000", passos="5", hover=HOVER["generator"]["slot_energia"]),
        caso("cubo_slot_descarga_antes", "cubo", energia="0", ranhura="mekanism:energy_tablet:600000", passos="1"),
        caso("cubo_slot_descarga_depois", "cubo", energia="0", ranhura="mekanism:energy_tablet:600000", passos="100"),
        caso("cubo_slot_descarga_hover", "cubo", energia="0", ranhura="mekanism:energy_tablet:600000", passos="100", hover=HOVER["cubo"]["slot_descarga"]),
        caso("cubo_slot_carga_antes", "cubo", energia="1500000", carga="mekanism:energy_tablet:0", passos="1"),
        caso("cubo_slot_carga_depois", "cubo", energia="1500000", carga="mekanism:energy_tablet:0", passos="100"),
        caso("cubo_slot_carga_hover", "cubo", energia="1500000", carga="mekanism:energy_tablet:0", passos="100", hover=HOVER["cubo"]["slot_carga"]),
    ]


def casos_controle():
    """O controle da luz do mundo atrás da tela: uma fonte vanilla (`minecraft:light`) de nível 4, 8, 12 e 15 ao lado de uma máquina parada, nos dois
    jogos. Sem nenhum bloco do Mekanism emitindo, a diferença do mundo que sobra é de como o jogo desenha a luz, não da luz do port."""
    controles = [caso(f"controle_luz_{n}", "smelter", energia="0", luz=str(n)) for n in (8, 4, 12, 15)]
    # O mesmo mundo sem tela aberta (`semtela`): com ele, a luz e o gamma de cada jogo aparecem sem o escurecimento da tela por cima.
    controles += [caso(f"controle_luz_{n}_semtela", "smelter", energia="0", luz=str(n), semtela="1") for n in (8, 15)]
    controles += [caso("controle_ativa_semtela", "smelter", energia="cap", entrada="raw_iron:2", passos="100", semtela="1"),
                  caso("controle_vazia_semtela", "smelter", energia="0", semtela="1")]
    return controles


def casos_fora():
    """Os estados do rastro de `GuiMekanism.hasClickedOutside` (`fora=1`, `rastros/fora.jsonl`): as quatro máquinas sem e com o upgrade de xp, sem e com a aba de
    avisos, e o Heat Generator parado e com lava. Só o cliente oficial os roda (`--so`): não são casos de pixel e ficam fora do lote do comparador."""
    out = []
    avisos = {
        "chamber": dict(energia="0", entrada="coal:2", passos="5"),
        "crusher": dict(energia="0", entrada="blaze_rod:2", passos="5"),
        "smelter": dict(energia="0", entrada="raw_iron:2", passos="5"),
        "infuser": dict(energia="0", entrada="clay_ball:2", quimico="bio:500", passos="5"),
    }
    for m, aviso in avisos.items():
        out += [
            caso(f"{m}_fora_base", m, energia="0", fora="1"),
            caso(f"{m}_fora_xp", m, energia="0", upgrades=XP, fora="1"),
            caso(f"{m}_fora_aviso", m, fora="1", **aviso),
            caso(f"{m}_fora_xp_aviso", m, fora="1", upgrades=XP, **aviso),
        ]
    out += [
        caso("generator_fora_base", "generator", energia="0", fora="1"),
        caso("generator_fora_lava", "generator", energia="0", lava="500", passos="100", fora="1"),
    ]
    return out


# Os avisos que o `CachedRecipe` não acende (`NOT_ENOUGH_INPUT` e `NOT_ENOUGH_SECONDARY_INPUT`: a entrada que não basta é receita que não casa, `mismatchedRecipe`, sem
# erro) e o overlay de nível >= 0,98 da barra de energia (`GuiBar.drawBackground`), que só o erro de energia de uma máquina pausada alcança, quando a energia chega
# fora do tick dela (`energia_depois`). `inativa=1`: a máquina não anda no estado, mesmo com energia e entrada (para a luz do mundo atrás da tela).
# `so_oficial=1`: só o cliente oficial roda o caso (hoje nenhum: o port escreve os data slots no `ServerTickEnd`, como o `broadcastChanges` do original os lê, e mostra
# o estado do overlay; antes de 2026-11-08 ele os escrevia no tick da máquina e o estado não chegava à tela do port).
FERRO = "raw_iron"


def casos_avisos():
    return [
        # A câmara com menos do que a receita pede (3 de ferro bruto): nenhum aviso, a tela é a de uma máquina parada. Com 3 ela anda.
        caso("chamber_ferro_curto", "chamber", energia="cap", entrada=f"{FERRO}:2", passos="5", inativa="1"),
        caso("chamber_ferro_exato", "chamber", energia="cap", entrada=f"{FERRO}:3", passos="5"),
        # O Infuser com menos químico do que a receita pede (10 de redstone com lingote de cobre): a barra de químico e o slot de entrada não piscam. Com 10 ele anda.
        # O item é plano (um bloco, como a terra, o Horizon desenha com outro sombreamento: H25), e cada tanque tem o ciclo de referência da animação do químico.
        caso("infuser_quimico_curto", "infuser", energia="cap", entrada="copper_ingot:2", quimico="redstone:9", passos="5", inativa="1"),
        caso("infuser_quimico_exato", "infuser", energia="cap", entrada="copper_ingot:2", quimico="redstone:10", passos="5"),
        caso("infuser_fase_redstone_9", "infuser", energia="0", quimico="redstone:9", ciclo="40", ciclo_port="40"),
        caso("infuser_fase_redstone_10", "infuser", energia="0", quimico="redstone:10", ciclo="40", ciclo_port="40"),
        # O erro de energia aceso (a barra pisca e a aba de avisos mostra o texto), para comparar com os de cima.
        caso("chamber_ferro_sem_energia", "chamber", energia="0", entrada=f"{FERRO}:3", passos="5"),
        # O overlay: velocidade 8 (consumo 500, capacidade 10000), energia 400 (erro aceso, máquina pausada), e 9999 que chega depois do tick: a barra a
        # 99,99 % com o erro velho aceso. O controle tem a mesma barra sem erro.
        caso("chamber_overlay_erro_velho", "chamber", upgrades="speed:8", energia="400", entrada="coal:2", passos="5", energia_depois="9999", inativa="1"),
        caso("chamber_overlay_sem_erro", "chamber", upgrades="speed:8", energia="9999", entrada="coal:2", passos="5", inativa="1"),
        caso("chamber_overlay_abaixo", "chamber", upgrades="speed:8", energia="400", entrada="coal:2", passos="5", energia_depois="9000", inativa="1"),
    ]



def casos_reabertas():
    """O mundo salvo e reaberto com a máquina como estava, e a tela aberta antes do primeiro tick do tile novo (`reabrir=1`): o `CenaTelas` grava o tile e o relê
    do NBT (`BlockEntity.loadStatic`), o port refaz o tile com o NBT que o jogo mostra (`monta_port`). O Java lê o tile direto, que o `loadAdditional` já carregou."""
    return [
        # A câmara operando (energia gasta, progresso da receita): a barra, a seta e o hover do progresso saem do NBT.
        caso("chamber_reaberta", "chamber", energia="cap", entrada="coal:4", passos="30", reabrir="1"),
        caso("chamber_reaberta_upgrades", "chamber", upgrades="speed:2", energia="800", entrada="coal:4", passos="30", reabrir="1"),
        # Sem energia e com a entrada: o tile reaberto não tem erro rastreado até o primeiro tick (os erros não são salvos), então a aba de avisos não aparece.
        caso("chamber_reaberta_sem_energia", "chamber", energia="0", entrada="coal:2", passos="5", reabrir="1"),
        caso("infuser_reaberta", "infuser", energia="cap", entrada="copper_ingot:2", quimico="redstone:10", passos="30", reabrir="1"),
        caso("generator_reaberto", "generator", energia="0", lava="1000", passos="10", reabrir="1"),
    ]


def casos_infuser_entrada():
    """O Infuser com item de entrada e sem químico: com e sem energia, e o item que nenhuma receita usa (a aba de avisos)."""
    return [
        caso("infuser_entrada_sem_nada", "infuser", energia="0", entrada="clay_ball:2", passos="5"),
        caso("infuser_entrada_cobre_sem_nada", "infuser", energia="0", entrada="copper_ingot:2", passos="5"),
        caso("infuser_entrada_sem_receita", "infuser", energia="0", entrada="stick:2", passos="5"),
        caso("infuser_entrada_sem_receita_com_energia", "infuser", energia="cap", entrada="stick:2", passos="5"),
    ]


# As abas de configuração de lados e de transporte (`GuiConfigurableTile`) e as duas janelas: o centro do botão de dentro de cada aba (-21, 10) e (-21, 38),
# 18 por 18, em pixels de GUI relativos ao canto da tela. As janelas abrem em ((176 - 156) / 2, 15) = (10, 15).
# As telas dos tanques (frente `tanques`, 5.ª rodada). O cursor: o meio do medidor largo do Fluid Tank (48, 18, 66 por 50), o botão da aba do modo de edição
# (179, 142, 18 por 18), a barra do Chemical Tank (42, 16, 118 por 12) e o botão do modo de descarte (159, 72, 10 por 10).
TANQUE_MEDIDOR = (81, 43)
TANQUE_ABA_MODO = (188, 151)
TANQUE_BARRA = (101, 22)
TANQUE_BOTAO = (164, 77)


def casos_tanques():
    """Os dois tanques: vazios, com fluido e com químico, o criativo, cada modo (posto no tile e pelo clique na tela) e os tooltips do medidor, da barra e da aba.
    O fluido e o químico são sprites animados nos dois jogos: `ciclo` tira vários prints, e `comparar_tanques.py` casa cada print do original com o do port mais parecido."""
    out = [
        caso("tanque_vazio", "tanque"),
        caso("tanque_agua", "tanque", fluido="water:16000", ciclo="4", ciclo_port="64"),
        caso("tanque_lava_cheio", "tanque", fluido="lava:cap", ciclo="2", ciclo_port="64"),
        caso("tanque_criativo_agua", "tanque_criativo", fluido="water:cap", ciclo="2", ciclo_port="64"),
        caso("tanque_modo_fill", "tanque", modo="fill"),
        caso("tanque_modo_empty", "tanque", modo="empty"),
        caso("tanque_clique_modo", "tanque", clicar="%d,%d" % TANQUE_ABA_MODO),
        caso("tanque_clique_modo_direito", "tanque", clicar="%d,%d,r" % TANQUE_ABA_MODO),
        caso("tanque_hover_medidor_vazio", "tanque", hover=TANQUE_MEDIDOR),
        caso("tanque_hover_medidor_agua", "tanque", fluido="water:16000", hover=TANQUE_MEDIDOR, ciclo="2", ciclo_port="64"),
        caso("tanque_hover_medidor_criativo", "tanque_criativo", fluido="water:cap", hover=TANQUE_MEDIDOR, ciclo="2", ciclo_port="64"),
        caso("tanque_hover_aba_modo", "tanque", hover=TANQUE_ABA_MODO),
        caso("tanque_hover_aba_modo_fill", "tanque", modo="fill", hover=TANQUE_ABA_MODO),
        caso("tanque_quimico_vazio", "tanque_quimico"),
        caso("tanque_quimico_hidrogenio", "tanque_quimico", quimico="hydrogen:5000", ciclo="4", ciclo_port="64"),
        caso("tanque_quimico_cheio", "tanque_quimico", quimico="oxygen:cap", ciclo="2", ciclo_port="64"),
        caso("tanque_quimico_criativo_vazio", "tanque_quimico_criativo"),
        caso("tanque_quimico_criativo_cheio", "tanque_quimico_criativo", quimico="hydrogen:cap", ciclo="2", ciclo_port="64"),
        caso("tanque_quimico_descarte_excesso", "tanque_quimico", descarte="dumping_excess"),
        caso("tanque_quimico_descarte", "tanque_quimico", descarte="dumping"),
        caso("tanque_quimico_clique_botao", "tanque_quimico", clicar="%d,%d" % TANQUE_BOTAO),
        caso("tanque_quimico_hover_botao", "tanque_quimico", hover=TANQUE_BOTAO),
        caso("tanque_quimico_hover_barra_vazia", "tanque_quimico", hover=TANQUE_BARRA),
        caso("tanque_quimico_hover_barra", "tanque_quimico", quimico="oxygen:5000", hover=TANQUE_BARRA, ciclo="2", ciclo_port="64"),
        caso("tanque_quimico_lados", "tanque_quimico", clicar=f"{ABA_LADOS[0]},{ABA_LADOS[1]}"),
    ]
    return out


ABA_LADOS = (-12, 19)
ABA_TRANSPORTE = (-12, 47)
JANELA = (10, 15)
ORDEM_DOS_LADOS = ("front", "left", "right", "back", "top", "bottom")
DATA_TYPE = {"none": 0, "input": 1, "input_1": 2, "input_2": 3, "output": 4, "output_1": 5, "output_2": 6, "input_output": 7, "energy": 8, "extra": 9}
TIPOS = {"energy": 0, "fluids": 1, "chemicals": 2, "items": 3, "heat": 4}
CORES = {"black": 0, "blue": 1, "green": 2, "cyan": 3, "dark_red": 4, "purple": 5, "orange": 6, "light_gray": 7, "gray": 8, "light_blue": 9, "lime": 10,
         "aqua": 11, "red": 12, "magenta": 13, "yellow": 14, "white": 15, "brown": 16, "pink": 17}


def lados(**tipos):
    """`lados=items:front=output,...;energy:...`: cada tipo com os seis lados, na ordem de `RelativeSide`."""
    return ";".join(f"{tipo}:" + ",".join(f"{lado}={valor}" for lado, valor in zip(ORDEM_DOS_LADOS, valores)) for tipo, valores in tipos.items())


def na_janela(x, y):
    """Um ponto relativo à janela (10, 15), em pixels de GUI relativos à tela."""
    return (JANELA[0] + x, JANELA[1] + y)


def casos_config():
    out = []
    for m in ("chamber", "crusher", "smelter", "infuser"):
        padrao = dict(items=("input", "input", "output", "energy", "input", "input"), energy=("input",) * 6)
        variado = dict(items=("output", "input_output", "energy", "none", "input", "output"), energy=("none", "input", "none", "input", "none", "input"))
        if m == "infuser":
            padrao["chemicals"] = ("input", "input", "output", "input", "input", "input")
            variado["chemicals"] = ("input", "none", "output", "input_output", "none", "output")
        indice_energia = len(padrao) - 1
        aba_energia = (JANELA[0] - 12, JANELA[1] + 15 + 28 * indice_energia)
        ejetar = dict(ejetar="items")
        transporte = dict(strict="1", cor_saida="red", cor_entrada="front:blue,left:yellow,top:dark_red")
        out += [
            caso(f"{m}_lados_padrao", m, energia="0", lados=lados(**padrao), clicar=f"{ABA_LADOS[0]},{ABA_LADOS[1]}"),
            caso(f"{m}_lados_variado", m, energia="0", lados=lados(**variado), clicar=f"{ABA_LADOS[0]},{ABA_LADOS[1]}", **ejetar),
            caso(f"{m}_lados_energia", m, energia="0", lados=lados(**variado), clicar=f"{ABA_LADOS[0]},{ABA_LADOS[1]};{aba_energia[0]},{aba_energia[1]}", **ejetar),
            caso(f"{m}_lados_hover_botao", m, energia="0", lados=lados(**variado), clicar=f"{ABA_LADOS[0]},{ABA_LADOS[1]}", hover=na_janela(101, 80), **ejetar),
            caso(f"{m}_lados_hover_limpar", m, energia="0", lados=lados(**variado), clicar=f"{ABA_LADOS[0]},{ABA_LADOS[1]}", hover=na_janela(143, 102), **ejetar),
            caso(f"{m}_transporte", m, energia="0", lados=lados(**variado), clicar=f"{ABA_TRANSPORTE[0]},{ABA_TRANSPORTE[1]}", **transporte),
            caso(f"{m}_transporte_hover_cor", m, energia="0", lados=lados(**variado), clicar=f"{ABA_TRANSPORTE[0]},{ABA_TRANSPORTE[1]}",
                 hover=na_janela(120, 57), **transporte),
            caso(f"{m}_duas_janelas", m, energia="0", lados=lados(**variado), clicar=f"{ABA_LADOS[0]},{ABA_LADOS[1]};{ABA_TRANSPORTE[0]},{ABA_TRANSPORTE[1]}",
                 **ejetar, **transporte),
        ]
    out.append(caso("infuser_lados_quimico", "infuser", energia="0",
                    lados=lados(items=("output", "input_output", "energy", "none", "input", "output"), chemicals=("input", "none", "output", "input_output", "none", "output"),
                                energy=("none", "input", "none", "input", "none", "input")),
                    clicar=f"{ABA_LADOS[0]},{ABA_LADOS[1]};{JANELA[0] - 12},{JANELA[1] + 15 + 28}"))
    return out


def nbt_da_configuracao(c):
    """O `component_config` e o `component_ejector` que o port recebe no lugar do que o `CenaTelas` põe com `configuracao`."""
    a = c["args"]
    if "lados" not in a:
        return []
    ejetando = set(a.get("ejetar", "").split(",")) - {""}
    partes = []
    for grupo in a["lados"].split(";"):
        tipo, entradas = grupo.split(":", 1)
        ids = [str(DATA_TYPE[e.split("=")[1]]) for e in entradas.split(",")]
        partes.append(f"config{TIPOS[tipo]}:[I;{','.join(ids)}]")
        partes.append(f"eject{TIPOS[tipo]}:{1 if tipo in ejetando else 0}b")
    saida = [f"component_config:{{{','.join(partes)}}}"]
    ejetor = []
    if "strict" in a:
        ejetor.append("strict_input:1b")
    if "cor_saida" in a:
        ejetor.append(f'color:"{a["cor_saida"]}"')
    if "cor_entrada" in a:
        cores = [-1] * 6
        for entrada in a["cor_entrada"].split(","):
            lado, cor = entrada.split(":")
            cores[ORDEM_DOS_LADOS.index(lado)] = CORES[cor]
        ejetor.append(f"input_color:[I;{','.join(str(v) for v in cores)}]")
    if ejetor:
        saida.append(f"component_ejector:{{{','.join(ejetor)}}}")
    return saida


def todos_os_casos():
    return casos() + casos_aba() + casos_config() + casos_xp() + casos_energia() + casos_gerador() + casos_avisos() + casos_reabertas() + casos_infuser_entrada() + casos_cubo() + casos_seguranca() + casos_redstone() + casos_livro()


def casos_do_roteiro(so=None):
    """Os casos que os dois jogos rodam: a tabela (`todos_os_casos`) e, só se `so` os pede pelo nome, os controles da luz (`casos_controle`), que não
    entram no lote porque comparam o mundo sem tela e a luz, não a tela."""
    tudo = todos_os_casos() + (casos_controle() + casos_fora() + casos_slot_energia() + casos_tanques() if so else [])
    return [c for c in tudo if not so or c["nome"] in so]


def maquina_ativa(c):
    """Se o bloco do caso está aceso (`lit`) quando o print sai: a máquina operando (entrada com receita, energia e saída com lugar) ou o gerador queimando lava
    (energia que ainda cabe). O bloco aceso ilumina o mundo atrás da tela: ver `luz_do_caso`."""
    a = c["args"]
    if int(a.get("passos", "2")) < 2 or "inativa" in a:
        return False
    if c["maquina"] == "generator":
        return "lava" in a and a.get("energia") != "cap"
    return "entrada" in a and "saida" not in a and a.get("energia", "0") != "0"


# O nível de luz do bloco aceso: `Attributes.ACTIVE_LIGHT` (8) nas quatro máquinas (`Machine.java:26`) e `ACTIVE_MELT_LIGHT` (13) no Heat Generator
# (`MekanismBlockTypes.java:474`); o bloco apagado emite 0. É o que o mundo atrás da tela mostra de diferente (`comparar_lote.LIMITE_MUNDO_POR_LUZ`).
LUZ_DA_MAQUINA_ACESA = 8
LUZ_DO_GERADOR_ACESO = 13


def luz_do_caso(c):
    """O nível de luz que o bloco emite no estado de `c`: 0 apagado, o da máquina ou o do gerador aceso."""
    if not maquina_ativa(c):
        return 0
    return LUZ_DO_GERADOR_ACESO if c["maquina"] == "generator" else LUZ_DA_MAQUINA_ACESA


def xp_visivel(c):
    """Se a aba de xp aparece no estado de `c`: o upgrade instalado (`upgrades`) ou o item no slot de upgrades com os 20 ticks de instalação
    já passados (`upgradeentrada` com `passos` de 20 ou mais)."""
    a = c["args"]
    return "xp_containment" in a.get("upgrades", "") or ("xp_containment" in a.get("upgradeentrada", "") and int(a.get("passos", "2")) >= 20)


def ciclos(c, lado):
    """Quantos prints de ciclo o `lado` (`oficial` ou `port`) tira em `c`: `ciclo`, ou `ciclo_port` no port quando o caso o pede."""
    ciclo = int(c["args"].get("ciclo", "0"))
    if lado == "port":
        return int(c["args"].get("ciclo_port", ciclo))
    return ciclo


def escreve_oficial(escalas, saida, so=None):
    linhas = []
    for escala in escalas:
        linhas.append(f"escala {escala}")
        for c in casos_do_roteiro(so):
            args = dict(c["args"])
            # `so_port=1`: o original não roda o caso (hoje nenhum).
            if "so_port" in args:
                continue
            if "hover" in args:
                args["hover"] = "%d,%d" % tuple(args["hover"])
            pares = " ".join(f"{k}={v}" for k, v in args.items())
            linhas.append(f"caso {c['nome']} {c['maquina']} {pares}".rstrip())
    with open(saida, "w") as arquivo:
        arquivo.write("\n".join(linhas) + "\n")


# O Horizon ----------------------------------------------------------------------------------------------------------------------

# `H_NAME=horizon2` roda no outro container (os caminhos de dentro dele são os mesmos).
CTL = (
    f"docker exec {os.environ.get('H_NAME', 'horizon')} sh -c 'l=$XDG_DATA_HOME/horizon-launcher; "
    "v=$(sed -n \"s/.*\\\"version\\\": *\\\"\\([^\\\"]*\\)\\\".*/\\1/p\" $l/instances.json | head -n 1); "
    'exec $l/versions/$v/horizon-ctl --timeout 60 "$@"\' sh'
)


def ctl(*args, check=True):
    comando = "ssh desktop " + shlex.quote(CTL + " " + " ".join(shlex.quote(a) for a in args))
    resultado = subprocess.run(comando, shell=True, capture_output=True, text=True)
    if check and resultado.returncode != 0:
        raise RuntimeError(f"{args}: {resultado.stdout} {resultado.stderr}")
    return resultado.stdout


def comando(texto):
    return ctl("command", texto)


def nbt_itens(c, maquina):
    itens = []
    entrada, saida = MAQUINAS[maquina]["slots"]
    for chave, slot in (("entrada", entrada), ("saida", saida)):
        if chave in c["args"]:
            item, n = c["args"][chave].rsplit(":", 1)
            if ":" not in item:
                item = "minecraft:" + item
            itens.append(f'{{Slot:{slot}b,id:"{item}",count:{n}}}')
    # Os itens dos dois slots de upgrade entram depois do tick em que a máquina nasce, como o `CenaTelas` (`state` roda depois do `tick step 1` da
    # montagem): um item que já está no slot quando o objeto nasce conta o tick do nascimento, e a seta de instalação sai um tick adiante (medido em 2026-11-08, `docs/plano-paridade.md`, item `MekanismContainer`, "Em jogo").
    # O slot de energia (`ranhura`, o que enche) e o de carga do cubo (`carga`): o item com a energia no componente `mekanism:energy`, como o handler dele a guarda.
    for chave, slot in (("ranhura", SLOT_DE_ENERGIA[MAQUINAS[maquina]["tela"]][0]), ("carga", SLOT_DE_ENERGIA[MAQUINAS[maquina]["tela"]][1])):
        if chave in c["args"]:
            item, energia = c["args"][chave].rsplit(":", 1)
            componentes = f',components:{{"mekanism:energy":{energia}L}}' if int(energia) else ""
            itens.append(f'{{Slot:{slot}b,id:"{item}",count:1{componentes}}}')
    entrada_up, saida_up = MAQUINAS[maquina]["upgrade"]
    for chave, slot in (("upgradeentrada", entrada_up), ("upgradesaida", saida_up)):
        if chave in c["args"]:
            tipo, n = c["args"][chave].split(":")
            itens.append(f'{{Slot:{slot}b,id:"mekanism:upgrade",count:{n},components:{{"mekanism:upgrade_type":"mekanism:{tipo}"}}}}')
    return itens


def componente_upgrade(c):
    a = c["args"]
    partes = []
    if "upgrades" in a:
        mapa = ",".join(f'"mekanism:{t}":{n}' for t, n in (p.split(":") for p in a["upgrades"].split(",")))
        partes.append(f"upgrades:{{{mapa}}}")
    return "{" + ",".join(partes) + "}" if partes else None


def passos_do_port(c):
    """Os passos que o port dá antes de abrir a tela: os `passos` do caso, como o `CenaTelas`. A janela de upgrades recebe os dados na abertura, sem tick depois."""
    passos = int(c["args"].get("passos", "2"))
    return passos


def monta_port(c, x, escala):
    m = c["maquina"]
    spec = MAQUINAS[m]
    pos = f"{x} -60 3"
    comando("tick freeze")
    # O mundo atrás da tela, igual ao do `CenaTelas`: uma caixa oca de concreto preto.
    comando(f"fill {x - 6} -61 -6 {x + 6} -50 10 minecraft:air")
    comando(f"fill {x - 6} -61 -6 {x + 6} -50 10 minecraft:black_concrete hollow")
    # O bioma decide a temperatura do ar (`CachedAmbientTemperature`), que a aba de calor mostra: planície nos dois jogos.
    comando(f"fillbiome {x - 6} -61 -6 {x + 6} -50 10 minecraft:plains")
    if "luz" in c["args"]:
        # O controle da luz: a fonte vanilla de nível `luz` ao lado da máquina, como no `CenaTelas`.
        comando(f"setblock {x + 2} -60 3 minecraft:light[level={c['args']['luz']}]")
    comando(f"setblock {pos} minecraft:air")
    comp = componente_upgrade(c)
    marca = comp is not None and m != "generator"
    if marca:
        # Um passo sem a máquina: o objeto vivo do caso anterior no mesmo lugar (o mundo parado só anda nos passos da cena) deixa de ser
        # do tick seguinte, e a máquina nova nasce do NBT em vez de continuar o objeto velho (ver a marca `redstone` abaixo).
        comando("tick step 1")
        ctl("wait", "4")
    # Como um jogador coloca a máquina (o `CenaTelas` chama `applyComponents`, que recalcula os upgrades e com eles a capacidade de energia): um
    # `setblock` deixaria a capacidade do construtor, quatro ticks de consumo, que é o que o original também faz para um bloco que não foi posto.
    ctl("tp", f"{x}.0", "-60.0", "0.0")
    comando("clear @s")
    ctl("give", spec["block"], "1")
    saida = ctl("place", str(x), "-60", "3", "--item", spec["block"])
    if '"error"' in saida:
        raise RuntimeError(f"place {spec['block']}: {saida}")
    comando("clear @s")
    # Os upgrades entram antes do primeiro tick: o objeto vivo os lê do NBT ao nascer, como o `deserialize`. O `CenaTelas` aplica os componentes
    # vazios e só depois instala (`addUpgrades` e os dois slots). No port, a máquina posta por um item aplica os componentes do item ao nascer
    # (`apply_item_components`), o que apagaria o que entrou aqui; com a marca de tile já gravado (`redstone`, que o Java escreve sempre) ela nasce
    # do NBT, e o `deserialize` recalcula cada upgrade suportado: o mesmo estado do original. O Heat Generator não tem objeto vivo e lê o NBT a cada tick.
    if comp:
        comando(f"data merge block {pos} {{component_upgrade:{comp}{',redstone:0b' if marca else ''}}}")
    # Um tick para o bloco nascer e a capacidade valer, como no `CenaTelas`.
    comando("tick step 1")
    ctl("wait", "4")
    nbt = []
    energia = c["args"].get("energia")
    if energia is not None and m == "cubo_criativo":
        # O criativo guarda o valor exato em `energy_amount` (decisão 67): o Horizon vê um buffer vazio.
        nbt.append("energy_amount:%dL" % (spec["cap"] if energia == "cap" else int(energia)))
    elif energia is not None:
        nbt.append("energy:%d" % (spec["cap"] if energia == "cap" else int(energia)))
    itens = nbt_itens(c, m)
    if itens:
        nbt.append("Items:[" + ",".join(itens) + "]")
    if "quimico" in c["args"]:
        tipo, n = c["args"]["quimico"].split(":")
        n = spec["cap"] if n == "cap" else n
        nbt.append(f'chemical_tanks:[{{container:0b,stored:{{type:"mekanism:{tipo}",amount:{n}L}}}}]')
    if "lava" in c["args"]:
        nbt.append(f'machinery_basis:{{v:1,tanks:[{{resource:"minecraft:lava",amount:{c["args"]["lava"]}L}}]}}')
    if "fluido" in c["args"]:
        # O Fluid Tank: o fluido no tanque da `machinery_basis` e, com `modo`, o modo de edição, nas chaves do tile.
        tipo, n = c["args"]["fluido"].split(":")
        nbt.append(f'machinery_basis:{{v:1,tanks:[{{resource:"minecraft:{tipo}",amount:{spec["cap"] if n == "cap" else int(n)}L}}]}}')
    if "modo" in c["args"]:
        nbt.append(f'edit_mode:"{c["args"]["modo"]}"')
    if "descarte" in c["args"]:
        nbt.append(f'dumping:"{c["args"]["descarte"]}"')
    nbt += nbt_da_configuracao(c)
    if "nome" in c["args"]:
        # O `CustomName` do tile é o componente em NBT (`ComponentSerialization`): o JSON sem espaços é também SNBT.
        nbt.append("CustomName:" + c["args"]["nome"])
    if nbt:
        comando(f"data merge block {pos} {{{','.join(nbt)}}}")

    ctl("tp", f"{x}.0", "-60.0", "0.0")
    passos = passos_do_port(c)
    comando(f"tick step {passos}")
    ctl("wait", str(passos + 4))
    if "energia_depois" in c["args"]:
        # A energia chega depois dos passos, fora do tick da máquina, como no `CenaTelas`: o `data merge` muda a energia que o tile guarda e o
        # `ServerTickEnd` seguinte (o servidor parado também tica o fim do tick) a leva aos data slots, com o erro velho ainda aceso.
        comando(f"data merge block {pos} {{energy:{int(c['args']['energia_depois'])}}}")
        ctl("wait", "6")
    if "reabrir" in c["args"]:
        reabre_no_port(pos, spec["block"])
    # O livro de receitas com tudo desbloqueado, como o `CenaTelas`: o `/recipe give @a *` chega aos displays do Mekanism pela injeção
    # `recipe_command` (decisão 263). Os outros casos do livro tiram tudo (`/recipe take @a *`), como o `CenaTelas`.
    if "receitas" in c["args"]:
        comando("recipe give @a *")
    elif "_livro_" in c["nome"]:
        comando("recipe take @a *")
    # O inventário de cada caso do livro começa vazio (o `clear` de cima) e leva o que `itens` diz, como no `CenaTelas`.
    for item in filter(None, c["args"].get("itens", "").split(",")):
        da_item(item)
    return x


def da_item(spec):
    """`slot:item:quantidade` (ou `slot:ns:item:quantidade`) no inventário de quem joga, como o `giveItem` do `CenaTelas`."""
    partes = spec.split(":")
    item = f"{partes[1]}:{partes[2]}" if len(partes) == 4 else f"minecraft:{partes[1]}"
    comando(f"item replace entity @s container.{partes[0]} with {item} {partes[-1]}")


def nbt_do_tile(pos):
    """O NBT do tile como o `data get block` o mostra (o `saveWithFullMetadata` do original), sem o `data get ... has the following block data:`."""
    chat = json.loads(comando(f"data get block {pos}"))["chat"][0]
    return chat.split("block data: ", 1)[1]


def sem_chaves_do_topo(snbt, chaves):
    """O mesmo composto sem as `chaves` do nível de cima: `x`, `y`, `z` e `id` dizem onde o tile está e o que ele é, e o `setblock` já os dá."""
    corpo = snbt.strip()[1:-1]
    partes, fundo, inicio, aspas = [], 0, 0, None
    for i, ch in enumerate(corpo):
        if aspas:
            if ch == aspas and corpo[i - 1] != "\\":
                aspas = None
        elif ch in "\"'":
            aspas = ch
        elif ch in "{[":
            fundo += 1
        elif ch in "}]":
            fundo -= 1
        elif ch == "," and fundo == 0:
            partes.append(corpo[inicio:i])
            inicio = i + 1
    partes.append(corpo[inicio:])
    return "{" + ",".join(p for p in partes if p.split(":", 1)[0].strip() not in chaves) + "}"


def reabre_no_port(pos, bloco):
    """O mundo salvo e reaberto com a máquina como está (`reabrir=1`): o tile sai e outro nasce do mesmo NBT, com o servidor parado, então ele ainda não andou
    quando a tela abre. O objeto vivo do tile velho segue na memória do mod, e o menu não o lê (o tile novo não tem a marca do tick)."""
    nbt = sem_chaves_do_topo(nbt_do_tile(pos), ("x", "y", "z", "id"))
    comando(f"setblock {pos} minecraft:air")
    comando(f"setblock {pos} {bloco}")
    comando(f"data merge block {pos} {nbt}")


def caminho_no_desktop(caminho):
    """O caminho do arquivo no desktop: dentro do container `horizon2` a pasta de dados aparece com o nome da do `horizon`."""
    return caminho.replace("/jogo-isolado/horizon/", f"/jogo-isolado/{os.environ.get('H_NAME', 'horizon')}/")


def iguais_no_recorte(a, b, recorte):
    """Se os dois prints são iguais dentro do `recorte` (esquerda, topo, direita, base, em pixels da imagem), comparado no desktop (PIL). O que o Energy Cube desenha
    no mundo atrás da tela (o núcleo gira com o `partial` do cliente, que o Horizon não prende com o `tick freeze`) fica fora do recorte, que é a tela e as abas; o recorte aceita 1 nível de diferença por canal, o ruído do mundo escuro entre a barra de armadura e o painel do gerador."""
    codigo = ("from PIL import Image, ImageChops; import sys; a, b = (Image.open(p).convert('RGB').crop(%r) for p in sys.argv[1:3]); "
              "print('igual' if max(e[1] for e in ImageChops.difference(a, b).getextrema()) <= 1 else 'diferente')" % (tuple(recorte),))
    a, b = caminho_no_desktop(a), caminho_no_desktop(b)
    saida = subprocess.run(["ssh", "desktop", f"python3 -c {shlex.quote(codigo)} {shlex.quote(a)} {shlex.quote(b)}"], capture_output=True, text=True).stdout.strip()
    return saida == "igual"


def espera_estado_estavel(alvo, destino, tentativas=12, recorte=None):
    """Tira o print até dois seguidos serem idênticos: a janela de upgrades recebe os dados do servidor alguns quadros depois de abrir
    (o servidor só os escreve com a janela acompanhada), e um print na frente disso mostra a lista vazia. Em vez de repetir o caso até
    passar, espera o estado parar de mudar e grava o último print. O arquivo é lido no desktop, por `md5sum`."""
    anterior = f"{destino}/.estavel.png"
    for _ in range(tentativas):
        ctl("screenshot", anterior)
        ctl("wait", "3")
        ctl("screenshot", alvo)
        if recorte is not None:
            if iguais_no_recorte(anterior, alvo, recorte):
                return
        else:
            soma = subprocess.run(["ssh", "desktop", f"md5sum {shlex.quote(caminho_no_desktop(anterior))} {shlex.quote(caminho_no_desktop(alvo))}"], capture_output=True, text=True).stdout.split()
            if len(soma) == 4 and soma[0] == soma[2]:
                return
        ctl("wait", "5")
    raise RuntimeError(f"o estado de {alvo} não parou de mudar em {tentativas} prints")


def tira_port(c, x, escala, destino):
    ctl("look", "0", "0")
    if "semtela" in c["args"]:
        # Sem tela: o print do mundo, na mesma vista.
        ctl("wait", "20")
        ctl("screenshot", f"{destino}/port_{c['nome']}_e{escala}.png")
        return
    ctl("interact", str(x), "-60", "3")
    ctl("wait", "60")
    # O `interact` vira o jogador para o bloco (inclinação 20, giro -9 no teste), e o `CenaTelas` fica de frente (`tp ... 0 0`): sem isto o
    # mundo atrás da tela é outra vista (a máquina some atrás do painel) e a comparação do mundo não vale.
    ctl("look", "0", "0")
    # `centered_origin`: a janela em pixels de GUI arredondada para cima, e a tela truncada no centro.
    gui_l = -(-1920 // escala)
    gui_a = -(-1080 // escala)
    esquerda = (gui_l - 176) // 2
    topo = (gui_a - 166) // 2
    # O livro com o Ctrl apertado: o `SlotSelectTime` não anda (`if (!hasControlDown()) time += a`, nos dois jogos) e todo botão de várias
    # receitas mostra o primeiro item, como nas primeiras capturas do original (os ciclos de 2, 3 e 4 itens só voltam juntos à mesma fase
    # depois de uma volta inteira, e os comandos do port por ssh levam mais que os 30 ticks da primeira troca). Só nos casos sem teclas,
    # porque na caixa de busca o Ctrl muda o que a tecla faz.
    ctrl_do_livro = "_livro_" in c["nome"] and "k=" not in c["args"].get("clicar", "")
    if ctrl_do_livro:
        ctl("key", "ctrl", "down")
    for clique in filter(None, c["args"].get("clicar", "").split(";")):
        # Entre os cliques, como no `CenaTelas`: `t=texto` digita (o `_` vale um espaço), `k=enter` aperta o Enter, `i=slot:item:quantidade`
        # põe um item no inventário; `x,y,r` é o botão direito e `x,y,s` o esquerdo com o shift.
        if clique.startswith("t="):
            ctl("type", clique[2:].replace("_", " "))
        elif clique.startswith("k="):
            # `k=tecla` ou `k=mods-tecla` (`s` o shift, `c` o ctrl), como no `CenaTelas`.
            mods, _, tecla = clique[2:].rpartition("-")
            seguradas = [nome for letra, nome in (("s", "shift"), ("c", "ctrl")) if letra in mods]
            for nome in seguradas:
                ctl("key", nome, "down")
            ctl("key", tecla, "tap")
            for nome in seguradas:
                ctl("key", nome, "up")
        elif clique.startswith("d="):
            # `d=x1,y1,x2,y2`: o esquerdo apertado no primeiro ponto, arrastado até o segundo e solto.
            x1, y1, x2, y2 = (int(v) for v in clique[2:].split(","))
            ctl("drag-at", str(esquerda + x1), str(topo + y1), str(esquerda + x2), str(topo + y2))
        elif clique.startswith("i="):
            da_item(clique[2:])
        else:
            partes = clique.split(",")
            cx, cy = int(partes[0]), int(partes[1])
            modo = partes[2] if len(partes) > 2 else ""
            if modo == "s":
                ctl("key", "shift", "down")
            ctl("click-at", str(esquerda + cx), str(topo + cy), *(["right"] if modo == "r" else []))
            if modo == "s":
                ctl("key", "shift", "up")
        ctl("wait", "6")
    if "digitar" in c["args"]:
        ctl("type", c["args"]["digitar"])
        ctl("wait", "4")
    if "hover" in c["args"]:
        # O Horizon dá o cursor em pixels de GUI da janela: o canto da tela mais o ponto dentro dela.
        hx, hy = c["args"]["hover"]
        ctl("scroll-at", str(esquerda + hx), str(topo + hy), "0")
    else:
        ctl("scroll-at", "2", "2", "0")
    ctl("wait", "10")
    ciclo = ciclos(c, "port")
    # `capturas_port`: só o port tira várias (o livro troca o item dos botões de várias receitas a cada 30 ticks, `SlotSelectTime`, e o
    # comparador escolhe a captura da mesma fase do original).
    capturas = ciclo or int(c["args"].get("capturas_port", c["args"].get("capturas", "1")))
    for k in range(capturas):
        sufixo = f"_{k}" if capturas > 1 else ""
        if ciclo:
            # Um passo do servidor por quadro, como o `CenaTelas`: a fase de cada print é conhecida.
            comando("tick step 1")
            ctl("wait", "2")
        alvo = f"{destino}/port_{c['nome']}_e{escala}{sufixo}.png"
        if c["args"].get("clicar") and c["maquina"].startswith("tanque"):
            # Os tanques: o texto do modo de descarte (`Dumping Excess`) não cabe ao lado do botão e rola nos dois jogos, e a tela nunca fica parada. O clique
            # já foi ao servidor e voltou no espelho do menu depois de 20 quadros.
            ctl("wait", "20")
            ctl("screenshot", alvo)
        elif c["args"].get("clicar"):
            # O cubo: só a tela e as abas valem (o núcleo atrás gira).
            recorte = ((gui_l - 176) // 2 - 28, (gui_a - 166) // 2, (gui_l - 176) // 2 + 204, (gui_a - 166) // 2 + 166) if c["maquina"].startswith("cubo") or c["maquina"] == "generator" else None
            if recorte:
                recorte = tuple(v * escala for v in recorte)
            espera_estado_estavel(alvo, destino, recorte=recorte)
        else:
            ctl("screenshot", alvo)
        if capturas > 1 and not ciclo:
            ctl("wait", "3")
    for clique in filter(None, c["args"].get("desfazer", "").split(";")):
        cx, cy = (int(v) for v in clique.split(","))
        ctl("click-at", str(esquerda + cx), str(topo + cy))
        ctl("wait", "3")
    if ctrl_do_livro:
        ctl("key", "ctrl", "up")
    ctl("close")


# As opções do cliente do original (`options.txt` da run do `mekanism-ref`) que mexem na luz, na cor e no que se vê do mundo. O Horizon nasce com
# outras (partículas decrescidas, distância 12, predefinição personalizada, sem vinheta, sem filtragem de textura): a predefinição `fancy` vai primeiro,
# porque ela reescreve as outras, e cada opção gráfica que o Horizon expõe separada vai depois, com o valor do original.
OPCOES_DO_ORIGINAL = (
    ("graphicsPreset", "fancy"), ("gamma", "0.5"), ("ao", "true"), ("biomeBlendRadius", "2"), ("darknessEffectScale", "1.0"), ("particles", "0"),
    ("renderDistance", "16"), ("entityShadows", "true"), ("cutoutLeaves", "true"), ("renderClouds", "true"), ("cloudRange", "64"),
    ("improvedTransparency", "false"), ("mipmapLevels", "4"), ("weatherRadius", "10"), ("vignette", "true"), ("textureFiltering", "1"),
    ("maxAnisotropyBit", "1"), ("prioritizeChunkUpdates", "1"), ("maxFps", "120"), ("menuBackgroundBlurriness", "5"), ("fov", "70"),
)


def iguala_as_opcoes():
    """Põe o cliente do Horizon nas opções do original e confere, com `options-get`, que cada uma está aplicada (e que nada ficou em `custom`)."""
    for chave, valor in OPCOES_DO_ORIGINAL:
        ctl("options-set", chave, valor)
    lidas = json.loads(ctl("options-get", *(chave for chave, _ in OPCOES_DO_ORIGINAL)))["options"]
    for chave, valor in OPCOES_DO_ORIGINAL:
        lido = str(lidas[chave]["value"]).lower()
        if lido != valor and not (chave in ("gamma", "darknessEffectScale") and float(lido) == float(valor)):
            raise RuntimeError(f"a opção {chave} ficou {lido}, e o original usa {valor}")


def roda_port(destino, escalas, so=None):
    # O mesmo inventário vazio do `CenaTelas`.
    comando("clear @s")
    iguala_as_opcoes()
    # O que o `CenaTelas` faz antes do primeiro caso (`difficulty peaceful`, `time set 6000`, `weather clear`): as mesmas entradas do mapa de luz nos dois jogos.
    comando("difficulty peaceful")
    comando("time set 6000")
    comando("weather clear")
    # Clima limpo e meio-dia com os ciclos parados (as gamerules voltam ao fim).
    comando("gamerule advance_time false")
    comando("gamerule advance_weather false")
    # O chat escondido, como o do `CenaTelas`, que não tem chat: as respostas dos comandos da montagem ficam na tela por 10 segundos, e a
    # guiScale 3 as põe dentro da margem das abas, onde contariam como o mundo atrás da tela.
    ctl("options-set", "chatVisibility", "2")
    for escala in escalas:
        ctl("options-set", "guiScale", str(escala))
        x = 3
        for c in casos_do_roteiro(so):
            if "so_oficial" in c["args"]:
                continue
            monta_port(c, x, escala)
            tira_port(c, x, escala, destino)
            print(c["nome"], escala, flush=True)
    comando("tick unfreeze")
    comando("gamerule advance_time true")
    comando("gamerule advance_weather true")
    ctl("options-set", "chatVisibility", "0")


if __name__ == "__main__":
    modo = sys.argv[1]
    if modo == "oficial":
        args = sys.argv[2:]
        so = None
        if "--so" in args:
            i = args.index("--so")
            so = args[i + 1].split(",")
            del args[i : i + 2]
        escreve_oficial([int(e) for e in args[1:]], args[0], so)
    elif modo == "port":
        args = sys.argv[2:]
        so = None
        if "--so" in args:
            i = args.index("--so")
            so = args[i + 1].split(",")
            del args[i : i + 2]
        roda_port(args[0], [int(e) for e in args[1:]], so)
    else:
        raise SystemExit(__doc__)
