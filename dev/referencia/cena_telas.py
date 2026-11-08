#!/usr/bin/env python3
"""Os estados de tela que o original e o port mostram lado a lado, e o roteiro do port. A tabela está aqui. Duas saídas:

    cena_telas.py oficial ARQUIVO ESCALA... [--so NOME,NOME] escreve o arquivo `cena-telas` que `testes/CenaTelas.java` lê (um por escala)
    cena_telas.py port DESTINO ESCALA... [--so NOME,NOME]
                                      monta cada estado no Horizon pelo `horizon-ctl` e grava `port_<caso>_e<escala>.png` em DESTINO

O port roda no container `horizon` do desktop (`docker exec horizon`), com o mundo superplano aberto e o servidor parado (`tick
freeze`) enquanto o estado é montado, como o `CenaTelas`. Os nomes dos casos são os mesmos nos dois lados.
"""
import json
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
}

# O que sai do slot de saída "cheio" e o item que não é o da receita.
SAIDA_CHEIA = {"chamber": "clay_ball", "crusher": "gravel", "smelter": "glass", "infuser": "dirt"}


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


def todos_os_casos():
    return casos() + casos_aba() + casos_xp() + casos_energia() + casos_gerador() + casos_avisos() + casos_reabertas() + casos_infuser_entrada()


def casos_do_roteiro(so=None):
    """Os casos que os dois jogos rodam: a tabela (`todos_os_casos`) e, só se `so` os pede pelo nome, os controles da luz (`casos_controle`), que não
    entram no lote porque comparam o mundo sem tela e a luz, não a tela."""
    tudo = todos_os_casos() + (casos_controle() + casos_fora() if so else [])
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
            if "hover" in args:
                args["hover"] = "%d,%d" % tuple(args["hover"])
            pares = " ".join(f"{k}={v}" for k, v in args.items())
            linhas.append(f"caso {c['nome']} {c['maquina']} {pares}".rstrip())
    with open(saida, "w") as arquivo:
        arquivo.write("\n".join(linhas) + "\n")


# O Horizon ----------------------------------------------------------------------------------------------------------------------

CTL = (
    "docker exec horizon sh -c 'l=$XDG_DATA_HOME/horizon-launcher; "
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
    if energia is not None:
        nbt.append("energy:%d" % (spec["cap"] if energia == "cap" else int(energia)))
    itens = nbt_itens(c, m)
    if itens:
        nbt.append("Items:[" + ",".join(itens) + "]")
    if "quimico" in c["args"]:
        tipo, n = c["args"]["quimico"].split(":")
        nbt.append(f'chemical_tanks:[{{container:0b,stored:{{type:"mekanism:{tipo}",amount:{n}L}}}}]')
    if "lava" in c["args"]:
        nbt.append(f'machinery_basis:{{v:1,tanks:[{{resource:"minecraft:lava",amount:{c["args"]["lava"]}L}}]}}')
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
    return x


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


def espera_estado_estavel(alvo, destino, tentativas=12):
    """Tira o print até dois seguidos serem idênticos: a janela de upgrades recebe os dados do servidor alguns quadros depois de abrir
    (o servidor só os escreve com a janela acompanhada), e um print na frente disso mostra a lista vazia. Em vez de repetir o caso até
    passar, espera o estado parar de mudar e grava o último print. O arquivo é lido no desktop, por `md5sum`."""
    anterior = f"{destino}/.estavel.png"
    for _ in range(tentativas):
        ctl("screenshot", anterior)
        ctl("wait", "3")
        ctl("screenshot", alvo)
        soma = subprocess.run(["ssh", "desktop", f"md5sum {shlex.quote(anterior)} {shlex.quote(alvo)}"], capture_output=True, text=True).stdout.split()
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
    for clique in filter(None, c["args"].get("clicar", "").split(";")):
        cx, cy = (int(v) for v in clique.split(","))
        ctl("click-at", str(esquerda + cx), str(topo + cy))
        ctl("wait", "6")
    if "hover" in c["args"]:
        # O Horizon dá o cursor em pixels de GUI da janela: o canto da tela mais o ponto dentro dela.
        hx, hy = c["args"]["hover"]
        ctl("scroll-at", str(esquerda + hx), str(topo + hy), "0")
    else:
        ctl("scroll-at", "2", "2", "0")
    ctl("wait", "10")
    ciclo = ciclos(c, "port")
    capturas = ciclo or int(c["args"].get("capturas", "1"))
    for k in range(capturas):
        sufixo = f"_{k}" if capturas > 1 else ""
        if ciclo:
            # Um passo do servidor por quadro, como o `CenaTelas`: a fase de cada print é conhecida.
            comando("tick step 1")
            ctl("wait", "2")
        alvo = f"{destino}/port_{c['nome']}_e{escala}{sufixo}.png"
        if c["args"].get("clicar"):
            espera_estado_estavel(alvo, destino)
        else:
            ctl("screenshot", alvo)
        if capturas > 1 and not ciclo:
            ctl("wait", "3")
    for clique in filter(None, c["args"].get("desfazer", "").split(";")):
        cx, cy = (int(v) for v in clique.split(","))
        ctl("click-at", str(esquerda + cx), str(topo + cy))
        ctl("wait", "3")
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
