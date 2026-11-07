#!/usr/bin/env python3
"""Compara a tela de uma máquina no original e no port, pixel a pixel.

Recebe dois prints da janela inteira, tirados com o mesmo `guiScale` e do mesmo tamanho, recorta o retângulo da tela (176 por 166 em
pixels de GUI), tira da conta as áreas das abas que o port ainda não tem e conta os pixels diferentes por elemento (barras, slots,
setas, medidor, rótulos e o resto, o fundo). Grava uma imagem de diferença. O alvo é zero pixels diferentes dentro da tela.

    comparar_tela.py ORIGINAL.png PORT.png --tela infuser --escala 2 --saida diferenca.png

O código de saída é 0 sem nenhuma diferença, 1 com alguma e 2 para entrada inválida (tamanhos diferentes, tela fora do print).
Só biblioteca padrão do Python: lê e grava PNG sozinho (8 bits, sem entrelaçamento, como os prints do jogo saem).
"""
import argparse
import json
import os
import struct
import sys
import zlib

# O retângulo da tela, em pixels de GUI (`AbstractContainerScreen.imageWidth` e `imageHeight`).
LARGURA = 176
ALTURA = 166

Rect = tuple  # (x, y, largura, altura), em pixels de GUI relativos ao canto de cima e da esquerda da tela

# A parte que toda tela de máquina tem (`GuiMekanism` e `AbstractContainerScreen`): o título centrado a 6 do topo, o rótulo do
# inventário a `imageHeight - 94` e os 36 slots do jogador, com a moldura a 7,83.
COMUNS = {
    "titulo": (4, 6, 168, 9),
    "rotulo_inventario": (8, 72, 162, 9),
    "inventario": (7, 83, 162, 76),
}

# Os quatro cantos do painel: o sprite `base` tem cantos transparentes, e o que se vê neles é o mundo atrás da tela. Contam à parte,
# porque o mundo é desenhado pelo renderer de cada jogo e o que sai ali não é do mod.
CANTOS = {
    "canto_cima_esquerda": (0, 0, 3, 3),
    "canto_cima_direita": (LARGURA - 3, 0, 3, 3),
    "canto_baixo_esquerda": (0, ALTURA - 3, 3, 3),
    "canto_baixo_direita": (LARGURA - 3, ALTURA - 3, 3, 3),
}

# Os elementos de cada tela, na posição do elemento do Java (a moldura de um slot começa um pixel antes do item).
TELAS = {
    # `GuiElectricMachine`: Enrichment Chamber, Crusher e Energized Smelter.
    "electric": {
        "slot_entrada": (63, 16, 18, 18),
        "slot_saida": (115, 34, 18, 18),
        "slot_energia": (63, 52, 18, 18),
        "seta": (68, 38, 8, 10),
        "progresso": (86, 38, 25, 9),
        "barra_energia": (164, 16, 6, 54),
    },
    # `GuiMetallurgicInfuser`.
    "infuser": {
        "slot_quimico": (16, 34, 18, 18),
        "slot_entrada": (50, 42, 18, 18),
        "slot_saida": (108, 42, 18, 18),
        "slot_energia": (142, 34, 18, 18),
        "progresso": (72, 47, 32, 8),
        "barra_energia": (164, 15, 6, 54),
        "barra_quimico": (7, 15, 6, 54),
        "botao_dump": (16, 59, 21, 10),
    },
    # `GuiHeatGenerator`.
    "generator": {
        "slot_combustivel": (16, 34, 18, 18),
        "slot_energia": (142, 34, 18, 18),
        "medidor_lava": (55, 18, 66, 50),
        "barra_energia": (164, 15, 6, 54),
    },
}

# A aba de avisos, que o port tem: à esquerda das máquinas (`GuiWarningTab`, -26,109) e à direita do gerador (`left = false`). Só
# aparece no recorte quando há margem em volta da tela.
ABA_AVISOS = {
    "electric": (-26, 109, 26, 26),
    "infuser": (-26, 109, 26, 26),
    "generator": (LARGURA, 109, 26, 26),
}

# A aba de upgrades (`GuiUpgradeWindowTab`: 26 por 26 em `imageWidth`, 6), que as máquinas elétricas e o Infuser têm.
ABA_UPGRADES = (LARGURA, 6, 26, 26)

# A aba de energia (`GuiEnergyTab`, -26 por 137, 26 por 26), que as quatro máquinas e o Heat Generator têm, e a de calor (`GuiHeatTab`, -26 por
# 109) do Heat Generator. São elementos de textura do original, não suportes: ficam visíveis na cena, e o que falta no port aparece como diferença.
ABA_ENERGIA = (-26, 137, 26, 26)
ABA_CALOR = (-26, 109, 26, 26)
# A barra de armadura do Heat Generator (`armorSideBar(-20, 11, 0)` no menu e `GuiSideHolder.create(-26, 6, 98, true, true, TAB_ARMOR_SLOTS)` na tela, portados em
# 2026-11-07): o suporte (`holder_left`, 26 por 98 em nove partes de borda 4, tingido), cinco `GuiSlot` de 18 por 18 em x = -21, y = 10, 28, 46, 64 e 82, e dentro de cada um
# o fundo vazio da peça (16 por 16, `InventoryMenu.EMPTY_ARMOR_SLOT_*`, em (-20, 11 + 18 i)). Valem nesta ordem (o primeiro que contém o pixel leva a contagem): o fundo
# vazio, o quadro do slot e o suporte. O `CenaTelas` deixa o suporte visível desde essa data (antes o ocultava com os outros).
SUPORTE_ARMADURA = (-26, 6, 26, 98)
SLOTS_ARMADURA = (-21, 10, 18, 90)
FUNDOS_ARMADURA = {f"fundo_armadura_{i}": (-20, 11 + 18 * i, 16, 16) for i in range(5)}
# O que o gerador desenha fora dos 176 por 166 e o recorte tem de incluir sempre, mesmo sem margem: sem isto um caso sem aba não vê a coluna.
FORA_DA_TELA = ("suporte_armadura",)

# A aba de xp (`GuiXpContainmentTab`: 26 por 26 em `imageWidth`, 90), que as máquinas de receita têm com o upgrade `xp_containment`. O botão de
# dentro, 18 por 18, começa em (179, 94).
ABA_XP = (LARGURA, 90, 26, 26)

# A janela de upgrades aberta no lugar padrão (`GuiUpgradeWindow` em (-11, 15), 198 por 94): cada filho no lugar dele, e a sombra em volta.
# Valem antes dos elementos da tela, porque a janela cobre os dois. Em pixels de GUI relativos ao canto da tela.
JANELA = {
    "janela_fechar": (-5, 21, 8, 8),
    "janela_fixar": (5, 21, 16, 8),
    "janela_titulo": (23, 20, 160, 12),
    # O ícone de cada linha da lista (`renderItem(..., relativeX + 3, relativeY + 3 + i * 12, 0.5F)`, 8 por 8): o Horizon escala o item depois
    # de montá-lo e o original sob uma matriz (H25), então este retângulo é um resíduo à parte, e o resto da lista conta sem desculpa.
    "janela_lista_icone_0": (-2, 36, 8, 8),
    "janela_lista_icone_1": (-2, 48, 8, 8),
    "janela_lista_icone_2": (-2, 60, 8, 8),
    "janela_lista_icone_3": (-2, 72, 8, 8),
    "janela_lista": (-5, 33, 108, 50),
    "janela_aceitos": (-5, 83, 167, 14),
    "janela_tela_interna": (103, 33, 59, 50),
    "janela_progresso_instalar": (165, 52, 10, 14),
    "janela_progresso_remover": (165, 73, 12, 12),
    "janela_uninstall": (104, 69, 56, 12),
    "janela_slot_entrada": (163, 32, 18, 18),
    "janela_slot_saida": (163, 86, 18, 18),
    # Por último: a primeira região que contém o pixel leva a contagem, e a sombra cobre a janela inteira.
    "janela_fundo": (-14, 12, 204, 100),
}

# As colunas onde as abas ficam, de cada lado da tela. O port ainda não tem as de configuração, transporte, cadeado, livro, energia,
# redstone e upgrades; as abas de avisos, de upgrades e de xp, que tem, saem da máscara.
COLUNA_ESQUERDA = (-26, -1, 26, ALTURA + 2)
COLUNA_DIREITA = (LARGURA, -1, 26, ALTURA + 2)

# O suporte de uma aba (`holder_right`, 26 por 9 em nove partes de borda 4) tem cantos transparentes, e o que se vê neles é o mundo atrás da
# tela, que cada jogo desenha do seu jeito (H22). Esses pixels contam à parte (`canto_aba_*`), como os do painel.
HOLDER = os.path.join(os.path.dirname(os.path.abspath(__file__)), "../../mods/mekanism/content/assets/mekanism/textures/gui/sprites/holder_right.png")
HOLDER_ESQUERDA = os.path.join(os.path.dirname(os.path.abspath(__file__)), "../../mods/mekanism/content/assets/mekanism/textures/gui/sprites/holder_left.png")
BORDA_SUPORTE = 4


CALOR_PNG = os.path.join(os.path.dirname(os.path.abspath(__file__)), "../../mods/mekanism/content/assets/mekanism/textures/gui/sprites/tab/heat_info_k.png")
ENERGIA_PNG = os.path.join(os.path.dirname(os.path.abspath(__file__)), "../../mods/mekanism/content/assets/mekanism/textures/gui/sprites/tab/energy_info.png")
# O fundo do tooltip é `0xF0100010` (94% opaco): o mundo atrás, que muda de um jogo para o outro (H22), aparece com até 6% de 255 em cada canal.
# Um texto que difere muda um canal em dezenas de níveis, então a região do tooltip tem esta tolerância e as outras nenhuma.
TOLERANCIA_TOOLTIP = 16
# O que o mundo atrás da tela mostra (o fundo fora do painel, os cantos transparentes, a sombra da janela) sai 2 a 3 níveis mais escuro no Horizon (H22).
# Essas áreas só valem como resíduo até este limite por canal: um pixel além dele (uma aba que só um dos jogos desenha, um texto do chat) é
# diferença de verdade e conta em `alem_<área>`.
CLASSES_DO_MUNDO = ("mundo_atras", "janela_fundo")


def cantos_do_sprite(rect, caminho):
    """Os pixels de GUI (relativos à tela) de `rect` em que o sprite `caminho`, do tamanho do retângulo e sem nove partes, é transparente."""
    try:
        largura, altura, rgba = ler_png(caminho)
    except OSError:
        return set()
    x, y, w, h = rect
    return {(x + dx, y + dy) for dy in range(min(h, altura)) for dx in range(min(w, largura)) if rgba[(dy * largura + dx) * 4 + 3] == 0}


def cantos_do_suporte(rect, caminho=HOLDER):
    """Os pixels de GUI (relativos à tela) do suporte `rect` (26 de largura) em que o sprite de nove partes `caminho` (`holder_right`, ou `holder_left` na barra de
    armadura) é transparente."""
    try:
        largura, altura, rgba = ler_png(caminho)
    except OSError:
        return set()
    x, y, w, h = rect
    transparentes = set()
    for dy in range(h):
        for dx in range(w):
            sx = dx if dx < BORDA_SUPORTE else (largura - (w - dx) if dx >= w - BORDA_SUPORTE else None)
            sy = dy if dy < BORDA_SUPORTE else (altura - (h - dy) if dy >= h - BORDA_SUPORTE else None)
            if sx is None or sy is None:
                continue
            if rgba[(sy * largura + sx) * 4 + 3] == 0:
                transparentes.add((x + dx, y + dy))
    return transparentes


def ler_png(caminho):
    """Lê um PNG de 8 bits sem entrelaçamento. Devolve (largura, altura, bytes RGBA, linha a linha em sequência)."""
    with open(caminho, "rb") as arquivo:
        dados = arquivo.read()
    if dados[:8] != b"\x89PNG\r\n\x1a\n":
        raise ValueError(f"{caminho}: não é um PNG")
    pos = 8
    idat = bytearray()
    paleta = None
    transparencia = None
    cabecalho = None
    while pos < len(dados):
        tamanho, tipo = struct.unpack(">I4s", dados[pos : pos + 8])
        corpo = dados[pos + 8 : pos + 8 + tamanho]
        pos += 12 + tamanho
        if tipo == b"IHDR":
            cabecalho = struct.unpack(">IIBBBBB", corpo)
        elif tipo == b"PLTE":
            paleta = corpo
        elif tipo == b"tRNS":
            transparencia = corpo
        elif tipo == b"IDAT":
            idat += corpo
        elif tipo == b"IEND":
            break
    if cabecalho is None:
        raise ValueError(f"{caminho}: sem cabeçalho")
    largura, altura, profundidade, cor, _, _, entrelacado = cabecalho
    paleta_curta = cor == 3 and profundidade in (1, 2, 4)
    if entrelacado != 0 or (profundidade != 8 and not paleta_curta) or cor not in (0, 2, 3, 4, 6):
        raise ValueError(f"{caminho}: só PNG de 8 bits sem entrelaçamento (profundidade {profundidade}, cor {cor})")
    canais = {0: 1, 2: 3, 3: 1, 4: 2, 6: 4}[cor]
    # Uma paleta de 1, 2 ou 4 bits guarda vários pixels por byte; o filtro ainda anda de byte em byte.
    passo = (largura * profundidade + 7) // 8 if paleta_curta else largura * canais
    bruto = zlib.decompress(bytes(idat))
    linhas = []
    anterior = bytearray(passo)
    posicao = 0
    for _ in range(altura):
        filtro = bruto[posicao]
        linha = bytearray(bruto[posicao + 1 : posicao + 1 + passo])
        posicao += 1 + passo
        if filtro == 1:
            for i in range(canais, passo):
                linha[i] = (linha[i] + linha[i - canais]) & 255
        elif filtro == 2:
            for i in range(passo):
                linha[i] = (linha[i] + anterior[i]) & 255
        elif filtro == 3:
            for i in range(passo):
                esquerda = linha[i - canais] if i >= canais else 0
                linha[i] = (linha[i] + ((esquerda + anterior[i]) >> 1)) & 255
        elif filtro == 4:
            for i in range(passo):
                a = linha[i - canais] if i >= canais else 0
                b = anterior[i]
                c = anterior[i - canais] if i >= canais else 0
                p = a + b - c
                pa, pb, pc = abs(p - a), abs(p - b), abs(p - c)
                previsao = a if pa <= pb and pa <= pc else (b if pb <= pc else c)
                linha[i] = (linha[i] + previsao) & 255
        elif filtro != 0:
            raise ValueError(f"{caminho}: filtro {filtro} desconhecido")
        linhas.append(linha)
        anterior = linha
    rgba = bytearray(largura * altura * 4)
    for y, linha in enumerate(linhas):
        base = y * largura * 4
        for x in range(largura):
            if cor == 6:
                pixel = linha[x * 4 : x * 4 + 4]
            elif cor == 2:
                pixel = bytes(linha[x * 3 : x * 3 + 3]) + b"\xff"
            elif cor == 0:
                pixel = bytes((linha[x],) * 3) + b"\xff"
            elif cor == 4:
                pixel = bytes((linha[x * 2],) * 3) + bytes((linha[x * 2 + 1],))
            else:
                if paleta_curta:
                    por_byte = 8 // profundidade
                    indice = (linha[x // por_byte] >> (8 - profundidade * (x % por_byte + 1))) & ((1 << profundidade) - 1)
                else:
                    indice = linha[x]
                alfa = transparencia[indice] if transparencia and indice < len(transparencia) else 255
                pixel = bytes(paleta[indice * 3 : indice * 3 + 3]) + bytes((alfa,))
            rgba[base + x * 4 : base + x * 4 + 4] = pixel
    return largura, altura, rgba


def gravar_png(caminho, largura, altura, rgba):
    """Grava RGBA de 8 bits."""

    def bloco(tipo, corpo):
        crc = zlib.crc32(tipo + corpo) & 0xFFFFFFFF
        return struct.pack(">I", len(corpo)) + tipo + corpo + struct.pack(">I", crc)

    linhas = bytearray()
    for y in range(altura):
        linhas.append(0)
        linhas += rgba[y * largura * 4 : (y + 1) * largura * 4]
    with open(caminho, "wb") as arquivo:
        arquivo.write(b"\x89PNG\r\n\x1a\n")
        arquivo.write(bloco(b"IHDR", struct.pack(">IIBBBBB", largura, altura, 8, 6, 0, 0, 0)))
        arquivo.write(bloco(b"IDAT", zlib.compress(bytes(linhas), 9)))
        arquivo.write(bloco(b"IEND", b""))


def origem_da_tela(largura, altura, escala):
    """O canto da tela no print, em pixels da imagem: `AbstractContainerScreen.init` centra no tamanho de GUI, que é a janela
    dividida pela escala arredondada para cima, e trunca."""
    gui_largura = -(-largura // escala)
    gui_altura = -(-altura // escala)
    return ((gui_largura - LARGURA) // 2) * escala, ((gui_altura - ALTURA) // 2) * escala


def elementos_da_tela(tela):
    """Os retângulos nomeados de uma tela, em pixels de GUI relativos ao canto dela, na ordem em que valem (o primeiro que
    contém o pixel leva a contagem)."""
    if tela not in TELAS:
        raise ValueError(f"tela desconhecida: {tela} (use {', '.join(sorted(TELAS))})")
    elementos = dict(TELAS[tela])
    elementos["aba_avisos"] = ABA_AVISOS[tela]
    elementos["aba_energia"] = ABA_ENERGIA
    if tela == "generator":
        elementos["aba_calor"] = ABA_CALOR
        elementos.update(FUNDOS_ARMADURA)
        elementos["slots_armadura"] = SLOTS_ARMADURA
        elementos["suporte_armadura"] = SUPORTE_ARMADURA
        # O Heat Generator aceita o upgrade de silêncio: a aba de upgrades fica à direita, como nas máquinas.
        elementos["aba_upgrades"] = ABA_UPGRADES
    if tela != "generator":
        elementos["aba_upgrades"] = ABA_UPGRADES
        elementos["aba_xp"] = ABA_XP
    elementos.update(COMUNS)
    elementos.update(CANTOS)
    return elementos


def dentro(rect, x, y):
    rx, ry, rw, rh = rect
    return rx <= x < rx + rw and ry <= y < ry + rh


def mascara_das_abas(tela):
    """O que sai da conta: as colunas das abas que o port não tem, menos a aba de avisos, que ele tem."""
    liberado = [ABA_AVISOS[tela], ABA_UPGRADES, ABA_ENERGIA]
    liberado += [ABA_XP] if tela != "generator" else [ABA_CALOR, SUPORTE_ARMADURA]
    return [COLUNA_ESQUERDA, COLUNA_DIREITA], liberado


def do_mundo(nome):
    return nome in CLASSES_DO_MUNDO or nome.startswith("canto")


def comparar(original, port, tela, escala, origem=None, margem=0, mascaras=(), tolerancia=0, regioes=None, mascarar_abas=True,
             prioridade=None, ausentes=(), limite_mundo=None, particulas=None):
    """Compara os dois prints (cada um `(largura, altura, rgba)`).

    Devolve (resultado, imagem), com `resultado` um dicionário com os totais e os elementos, e `imagem` o `(largura, altura, rgba)`
    do recorte com as diferenças em vermelho. `margem` inclui tantos pixels de GUI em volta da tela (onde as abas ficam)."""
    largura, altura, a = original
    largura_b, altura_b, b = port
    if (largura, altura) != (largura_b, altura_b):
        raise ValueError(f"os prints têm tamanhos diferentes: {largura}x{altura} e {largura_b}x{altura_b}")
    if escala < 1:
        raise ValueError("a escala é pelo menos 1")
    if origem is None:
        origem = origem_da_tela(largura, altura, escala)
    ox, oy = origem
    elementos = elementos_da_tela(tela)
    # Os elementos que o estado do caso não desenha (a aba de xp sem o upgrade): o que sobra no lugar deles é o mundo atrás da tela.
    for nome in ausentes:
        elementos.pop(nome, None)
    regioes = dict(regioes or {})
    prioridade = dict(prioridade or {})
    # O que tem prioridade conta antes dos elementos da tela e estende o recorte, para a janela caber inteira.
    regioes.update({nome: rect for nome, rect in prioridade.items() if nome not in regioes})
    mascara, liberado = mascara_das_abas(tela) if mascarar_abas else ([], [])
    mascara = list(mascara) + list(mascaras)
    caixa = (-margem, -margem, LARGURA + 2 * margem, ALTURA + 2 * margem)
    gui_esquerda, gui_topo = caixa[0], caixa[1]
    gui_direita, gui_base = caixa[0] + caixa[2], caixa[1] + caixa[3]
    for rx, ry, rw, rh in list(regioes.values()) + [elementos[nome] for nome in FORA_DA_TELA if nome in elementos]:
        gui_esquerda, gui_topo = min(gui_esquerda, rx), min(gui_topo, ry)
        gui_direita, gui_base = max(gui_direita, rx + rw), max(gui_base, ry + rh)
    esquerda, topo = ox + gui_esquerda * escala, oy + gui_topo * escala
    direita, base = ox + gui_direita * escala, oy + gui_base * escala
    if esquerda < 0 or topo < 0 or direita > largura or base > altura:
        raise ValueError(
            f"a tela não cabe no print: recorte {esquerda},{topo} a {direita},{base} num print de {largura}x{altura}"
        )
    recorte_largura, recorte_altura = direita - esquerda, base - topo
    saida = bytearray(recorte_largura * recorte_altura * 4)
    cantos = {"canto_aba_upgrades": cantos_do_suporte(ABA_UPGRADES), "canto_aba_xp": cantos_do_suporte(ABA_XP),
              "canto_aba_energia": cantos_do_sprite(ABA_ENERGIA, ENERGIA_PNG), "canto_aba_calor": cantos_do_sprite(ABA_CALOR, CALOR_PNG),
              "canto_suporte_armadura": cantos_do_suporte(SUPORTE_ARMADURA, HOLDER_ESQUERDA)}
    contagem = {nome: [0, 0] for nome in list(regioes) + list(elementos) + list(cantos)}  # [pixels comparados, pixels diferentes]
    contagem["fundo"] = [0, 0]
    contagem["mundo_atras"] = [0, 0]
    maior_do_elemento = {}  # a maior diferença por canal entre os pixels diferentes de cada elemento
    alem_amostra = []  # os primeiros pixels que passaram do limite do mundo: (área, x, y de GUI, cor no original, cor no port)
    maior_do_mundo = 0  # a maior diferença por canal que ficou dentro do limite do mundo: a medida do limite
    # `particulas`: o retângulo do print (x, y, largura, altura, em pixels da imagem) onde as partículas da máquina acesa caem; o que passa do `limite_mundo`
    # ali conta em `particulas_da_maquina`, à parte, e fora dali em `alem_*`.
    mascarados = 0
    ignorados = 0
    classe = {}  # o que cada pixel de GUI é: None se fica fora da conta, senão o nome do elemento

    def classificar(gx, gy):
        chave = (gx, gy)
        if chave not in classe:
            if any(dentro(m, gx, gy) for m in mascara) and not any(dentro(m, gx, gy) for m in liberado):
                classe[chave] = None
            else:
                nome = next((nome for nome, rect in prioridade.items() if dentro(rect, gx, gy)), None)
                if nome == "suporte_armadura" and (gx, gy) in cantos["canto_suporte_armadura"]:
                    # O suporte vindo da prioridade (sob a janela de upgrades) também tem os cantos transparentes, que mostram o mundo atrás.
                    nome = "canto_suporte_armadura"
                for aba, rect in (("canto_aba_upgrades", ABA_UPGRADES), ("canto_aba_xp", ABA_XP), ("canto_aba_energia", ABA_ENERGIA), ("canto_aba_calor", ABA_CALOR),
                                  ("canto_suporte_armadura", SUPORTE_ARMADURA)):
                    if nome is None and aba[len("canto_"):] in elementos and (gx, gy) in cantos[aba]:
                        nome = aba
                nome = nome or next((nome for nome, rect in elementos.items() if dentro(rect, gx, gy)), None)
                if nome is None:
                    # A região só vale fora da tela: dentro dela cada pixel é de um elemento ou do fundo.
                    tela_inteira = (0, 0, LARGURA, ALTURA)
                    if dentro(tela_inteira, gx, gy):
                        nome = "fundo"
                    else:
                        nome = next((nome for nome, rect in regioes.items() if dentro(rect, gx, gy)), None)
                        if nome is None:
                            nome = "mundo_atras" if dentro(caixa, gx, gy) else "fora"
                classe[chave] = nome
        return classe[chave]

    for py in range(recorte_altura):
        gy = (topo + py - oy) // escala
        for px in range(recorte_largura):
            gx = (esquerda + px - ox) // escala
            indice = ((topo + py) * largura + esquerda + px) * 4
            destino = (py * recorte_largura + px) * 4
            nome = classificar(gx, gy)
            if nome is None or nome == "fora":
                if nome is None:
                    mascarados += 1
                else:
                    ignorados += 1
                cinza = (a[indice] + a[indice + 1] + a[indice + 2]) // 12 + 40
                saida[destino : destino + 4] = bytes((cinza, cinza, cinza + 20, 255))
                continue
            contagem[nome][0] += 1
            limite = max(tolerancia, TOLERANCIA_TOOLTIP) if nome == "tooltip" else tolerancia
            if any(abs(a[indice + canal] - b[indice + canal]) > limite for canal in range(3)):
                conta = nome
                if limite_mundo is not None and do_mundo(nome):
                    maior = max(abs(a[indice + canal] - b[indice + canal]) for canal in range(3))
                    if maior > limite_mundo:
                        x_img, y_img = esquerda + px, topo + py
                        nas_particulas = particulas is not None and (
                            particulas[0] <= x_img < particulas[0] + particulas[2] and particulas[1] <= y_img < particulas[1] + particulas[3])
                        conta = "particulas_da_maquina" if nas_particulas else "alem_" + nome
                        contagem.setdefault(conta, [0, 0])[0] += 1
                        if not nas_particulas and len(alem_amostra) < 12:
                            alem_amostra.append((nome, gx, gy, tuple(a[indice : indice + 3]), tuple(b[indice : indice + 3])))
                    else:
                        maior_do_mundo = max(maior_do_mundo, maior)
                contagem[conta][1] += 1
                maior_do_elemento[conta] = max(maior_do_elemento.get(conta, 0), max(abs(a[indice + canal] - b[indice + canal]) for canal in range(3)))
                saida[destino : destino + 4] = b"\xff\x00\x00\xff"
            else:
                # O pixel igual sai apagado, para as diferenças aparecerem sobre o desenho da tela.
                saida[destino : destino + 4] = bytes((a[indice] // 3, a[indice + 1] // 3, a[indice + 2] // 3, 255))
    total = sum(diferentes for _, diferentes in contagem.values())
    comparados = sum(comparados for comparados, _ in contagem.values())
    resultado = {
        "tela": tela,
        "escala": escala,
        "origem": [ox, oy],
        "recorte": [esquerda, topo, recorte_largura, recorte_altura],
        "comparados": comparados,
        "mascarados": mascarados,
        "ignorados": ignorados,
        "diferentes": total,
        "maior_do_mundo": maior_do_mundo,
        "alem_amostra": alem_amostra,
        "elementos": {
            nome: {"rect": list(regioes.get(nome) or elementos.get(nome, ())), "comparados": c, "diferentes": d, "maior": maior_do_elemento.get(nome, 0)}
            for nome, (c, d) in contagem.items()
            if c or d
        },
    }
    return resultado, (recorte_largura, recorte_altura, saida)


def lado_a_lado(original, port, diferenca, recorte):
    """Três recortes lado a lado, com 8 pixels entre eles: o original, o port e a diferença. `recorte` é `[x, y, largura, altura]`."""
    x, y, w, h = recorte
    imagens = [original, port, diferenca]
    folga = 8
    largura = w * 3 + folga * 2
    saida = bytearray(b"\x20\x20\x20\xff" * (largura * h))
    for indice, imagem in enumerate(imagens):
        larg_img, _, rgba = imagem
        for linha in range(h):
            if indice < 2:
                origem = ((y + linha) * larg_img + x) * 4
            else:
                origem = linha * larg_img * 4
            destino = (linha * largura + indice * (w + folga)) * 4
            saida[destino : destino + w * 4] = rgba[origem : origem + w * 4]
    return largura, h, saida


def _rect(texto):
    partes = [int(valor) for valor in texto.split(",")]
    if len(partes) != 4:
        raise argparse.ArgumentTypeError("espere x,y,largura,altura")
    return tuple(partes)


def _par(texto):
    partes = [int(valor) for valor in texto.split(",")]
    if len(partes) != 2:
        raise argparse.ArgumentTypeError("espere x,y")
    return tuple(partes)


def principal(argv=None):
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("original", help="print do original")
    ap.add_argument("port", help="print do port, do mesmo tamanho e com o mesmo guiScale")
    ap.add_argument("--tela", required=True, choices=sorted(TELAS), help="que tela está nos prints")
    ap.add_argument("--escala", type=int, required=True, help="o guiScale dos dois prints")
    ap.add_argument("--origem", type=_par, help="o canto da tela no print, em pixels da imagem (padrão: centrada)")
    ap.add_argument("--margem", type=int, default=0, help="pixels de GUI em volta da tela a incluir (as abas)")
    ap.add_argument("--mascara", type=_rect, action="append", default=[], help="x,y,largura,altura em pixels de GUI da tela")
    ap.add_argument("--abas-ocultas", action="store_true",
                    help="o print do original foi tirado com as abas que o port não tem ocultas: não mascara as colunas delas")
    ap.add_argument("--tolerancia", type=int, default=0, help="diferença por canal que ainda conta como igual (padrão 0)")
    ap.add_argument("--regiao", action="append", default=[], metavar="NOME=x,y,largura,altura",
                    help="uma região fora da tela que também conta (o tooltip), em pixels de GUI relativos à tela")
    ap.add_argument("--saida", help="onde gravar a imagem de diferença")
    ap.add_argument("--lado-a-lado", help="onde gravar o original, o port e a diferença lado a lado")
    ap.add_argument("--json", action="store_true", help="imprime o resultado em JSON")
    args = ap.parse_args(argv)
    try:
        original = ler_png(args.original)
        port = ler_png(args.port)
        regioes = {}
        for texto in args.regiao:
            nome, _, rect = texto.partition("=")
            regioes[nome] = _rect(rect)
        resultado, imagem = comparar(
            original, port, args.tela, args.escala, args.origem, args.margem, args.mascara, args.tolerancia, regioes,
            not args.abas_ocultas,
        )
    except (OSError, ValueError, zlib.error) as erro:
        print(f"erro: {erro}", file=sys.stderr)
        return 2
    if args.saida:
        gravar_png(args.saida, *imagem)
    if args.lado_a_lado:
        gravar_png(args.lado_a_lado, *lado_a_lado(original, port, imagem, resultado["recorte"]))
    if args.json:
        print(json.dumps(resultado, indent=2, ensure_ascii=False))
    else:
        print(f"{resultado['tela']} em guiScale {resultado['escala']}, recorte {resultado['recorte']}")
        for nome, dados in resultado["elementos"].items():
            marca = "ok " if dados["diferentes"] == 0 else "DIF"
            print(f"  {marca} {nome:<18} {dados['diferentes']:>7} de {dados['comparados']}")
        print(
            f"total: {resultado['diferentes']} pixels diferentes de {resultado['comparados']} comparados "
            f"({resultado['mascarados']} fora, pelas abas)"
        )
    return 0 if resultado["diferentes"] == 0 else 1


if __name__ == "__main__":
    sys.exit(principal())
