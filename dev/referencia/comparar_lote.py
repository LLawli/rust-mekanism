#!/usr/bin/env python3
"""Compara todos os estados de `cena_telas.py` entre o original e o port.

    comparar_lote.py PASTA_ORIGINAL PASTA_PORT SAIDA ESCALA... [--so NOME,NOME]

`PASTA_ORIGINAL` tem os `oficial_<caso>_e<escala>[_<n>].png` do `CenaTelas`, `PASTA_PORT` os `port_<caso>_e<escala>[_<n>].png` do
`cena_telas.py port`. Grava em `SAIDA` um `tela_<caso>[_e<escala>].png` por estado (o original, o port e a diferença lado a lado: a
escala 2 não leva sufixo) e um `resultado.json`, e imprime os pixels diferentes por elemento. Sem tolerância e sem máscara além da
das abas, que o original da cena já traz ocultas. Código de saída 1 se algum estado tem diferença.
"""
import json
import os
import sys

import cena_telas
import comparar_tela as ct

def caixa_do_tooltip(imagem, escala, hover):
    """O retângulo do tooltip numa imagem, em pixels de GUI relativos à tela, ou `None` se não há. O fundo e a borda do tooltip são
    roxos (`0xF0100010` e o degradê de `0x505000FF`), o que nada mais na cena é; procura perto do cursor."""
    largura, altura, rgba = imagem
    ox, oy = ct.origem_da_tela(largura, altura, escala)
    hx, hy = hover
    x0, y0 = max(0, ox + (hx - 20) * escala), max(0, oy + (hy - 60) * escala)
    x1, y1 = min(largura, ox + (hx + 300) * escala), min(altura, oy + (hy + 80) * escala)
    achados = []
    for y in range(y0, y1):
        base = y * largura * 4
        for x in range(x0, x1):
            r, g, b = rgba[base + x * 4 : base + x * 4 + 3]
            if b >= g + 8 and r >= g + 8 and r < 140:
                achados.append((x, y))
    if not achados:
        return None
    xs = [p[0] for p in achados]
    ys = [p[1] for p in achados]
    esquerda, topo = (min(xs) - ox) // escala, (min(ys) - oy) // escala
    direita, base = -(-(max(xs) + 1 - ox) // escala), -(-(max(ys) + 1 - oy) // escala)
    return (esquerda, topo, direita - esquerda, base - topo)


def uniao(a, b):
    if a is None or b is None:
        return a or b
    x = min(a[0], b[0])
    y = min(a[1], b[1])
    return (x, y, max(a[0] + a[2], b[0] + b[2]) - x, max(a[1] + a[3], b[1] + b[3]) - y)


def pares(casos, escala, original, port):
    for c in casos:
        capturas = int(c["args"].get("ciclo", "0")) or int(c["args"].get("capturas", "1"))
        for k in range(capturas):
            sufixo = f"_{k}" if capturas > 1 else ""
            nome = f"{c['nome']}_e{escala}{sufixo}"
            yield c, k, nome, os.path.join(original, f"oficial_{nome}.png"), os.path.join(port, f"port_{nome}.png")


def recorte(imagem, escala, rect):
    """As linhas de pixels de um retângulo de GUI relativo à tela."""
    largura, altura, rgba = imagem
    ox, oy = ct.origem_da_tela(largura, altura, escala)
    x, y, w, h = rect
    return [bytes(rgba[((oy + (y * escala) + j) * largura + ox + x * escala) * 4 :][: w * escala * 4]) for j in range(h * escala)]


def diferentes(a, b):
    return sum(1 for ra, rb in zip(a, b) for i in range(0, len(ra), 4) if ra[i : i + 3] != rb[i : i + 3])


def casar_fases(c, escala, original, port):
    """Um ciclo de uma animação do atlas: o original e o port tiram `N` prints, um por passo do servidor, mas cada um conta o tempo da
    animação do seu jeito (o original desde o carregamento das texturas, e anda um quadro a cada dois passos), então as fases não
    coincidem print a print. O port, em `N` passos, passa por todos os estados do ciclo; cada print do original deve ser igual a algum
    deles. Devolve, para cada print do original, o print do port com menos pixels diferentes no elemento animado."""
    n = cena_telas.ciclos(c, "oficial")
    m = cena_telas.ciclos(c, "port")
    tela = cena_telas.MAQUINAS[c["maquina"]]["tela"]
    rect = ct.elementos_da_tela(tela)["barra_quimico" if tela == "infuser" else "medidor_lava"]
    arquivos = lambda lado, prefixo, total: [os.path.join(lado, f"{prefixo}_{c['nome']}_e{escala}_{k}.png") for k in range(total)]
    ori = [recorte(ct.ler_png(p), escala, rect) for p in arquivos(original, "oficial", n)]
    prt = [recorte(ct.ler_png(p), escala, rect) for p in arquivos(port, "port", m)]
    pares_ = []
    for k in range(n):
        custo, j = min((diferentes(ori[k], prt[j]), j) for j in range(m))
        pares_.append((custo, j))
    return pares_, arquivos(original, "oficial", n), arquivos(port, "port", m)


def regioes_do_caso(c, a, b, escala):
    """O tooltip do caso: a união das caixas que ele ocupa no original e no port, para que um tooltip que só um dos dois desenha
    também apareça como diferença."""
    if "hover" not in c["args"]:
        return {}
    caixa = uniao(caixa_do_tooltip(a, escala, c["args"]["hover"]), caixa_do_tooltip(b, escala, c["args"]["hover"]))
    return {"tooltip": caixa} if caixa else {}


# O que o Horizon desenha diferente do original e já está pedido a ele (docs/injecoes-horizon.md): o mundo atrás da tela (H22 o escurecimento, que a injeção
# `container_dim` já refaz a menos de arredondamento da GPU, e H30 a iluminação do mundo, mais clara no original e crescente com a luz), os cantos transparentes do painel e das abas, a sombra da janela sobre esse mundo, os ícones de item em escala (H25: os
# de "Supported:" e os das linhas da lista) e a fase do sprite animado do químico (H26, ver `fase_do_quimico`). O mundo vale até `LIMITE_MUNDO_POR_LUZ`:
# o que passa dele é `alem_*`, diferença de verdade, e o que fica dentro sai no relatório como `NE` (não explicado), não como `ok`.
RESIDUOS_DO_HORIZON = ("mundo_atras", "janela_fundo", "janela_aceitos", "barra_quimico_fase", "medidor_lava_fase")
ICONE_DA_LISTA = "janela_lista_icone"
PREFIXOS_RESIDUO = ("canto", ICONE_DA_LISTA, "tooltip_canto")
# O que o mundo atrás da tela pode diferir por canal, por nível de luz do bloco (`cena_telas.luz_do_caso`): 0 com a máquina parada, 8 com uma das quatro
# acesas, 13 com o Heat Generator aceso. Medido no lote, com a injeção `container_dim` no lugar (`docs/plano-paridade.md`, "Tetos"); o controle com fonte vanilla
# (`minecraft:light` de nível 4, 8, 12 e 15, máquina parada) deu 1 por canal em todos os níveis, com o escurecimento refeito: o que sobra é a iluminação do Horizon (H30) e o arredondamento do H22.
LIMITE_MUNDO_POR_LUZ = {0: 1, 8: 1, 13: 1}
# O que difere entre os dois jogos por construção, e que aparece no relatório como diferença conhecida (não conta como resíduo do Horizon nem fica calada): nome e causa.
DIFERENCAS_CONHECIDAS = {
    "particulas_da_maquina": "partículas da máquina acesa (AttributeParticleFX em Machine.java:23: fumaça e poeira de redstone na face da frente, a cada "
                             "animateTick ativo): portadas (injeção animate_tick, taxa e posição conferidas à parte), mas cada jogo sorteia as suas com um gerador sem semente "
                             "(ClientLevel.animateTick), então onde cai cada uma difere entre os dois prints por construção; também as que a máquina do caso anterior deixou no ar",
}
# Onde essas partículas caem no print (1920 por 1080, a vista de frente do `CenaTelas`, igual em todo guiScale): a face da frente da máquina (x 705 a 960, y 660
# a 955 nos prints medidos), com folga. Só nos estados em que a máquina está acesa ou o caso anterior estava (`com_particulas`).
CAIXA_DAS_PARTICULAS = (690, 640, 285, 330)
# O Horizon desenha a caixa do tooltip de item com cantos quadrados, e o original com os sprites de nove partes (H23): os cantos, de `CANTO_TOOLTIP` por
# `CANTO_TOOLTIP` pixels de GUI, são resíduo à parte, e o resto do tooltip (texto e fundo) conta com a tolerância do tooltip.
CANTO_TOOLTIP = 4
CASOS_COM_TOOLTIP_DE_ITEM = ("chamber_hover_slot_item",)


def ler_visibilidade(caminho):
    """As abas que o original diz ter (ou não) em cada caso, do log do `CenaTelas` (`cena: <caso> fora GuiWarningTab ... visivel false` e
    `cena: <caso> xp_tab visible true ...`). Sem o log devolve um dicionário vazio e `ausentes_do_caso` cai na regra por nome."""
    visivel = {}
    try:
        with open(caminho, errors="replace") as arquivo:
            for linha in arquivo:
                partes = linha.split("cena: ", 1)[-1].split()
                if len(partes) < 3:
                    continue
                caso = partes[0]
                if partes[1] == "fora" and partes[2] == "GuiWarningTab" and partes[-2] == "visivel":
                    visivel.setdefault(caso, {})["aba_avisos"] = partes[-1] == "true"
                elif partes[1] == "xp_tab" and partes[2] == "visible":
                    visivel.setdefault(caso, {})["aba_xp"] = partes[3] == "true"
    except OSError:
        pass
    return visivel


def ausentes_do_caso(c, visivel=None):
    """Os elementos que o original não desenha no estado de `c`: o que o log do `CenaTelas` diz (`visivel`), ou, sem ele, a aba de xp sem o
    upgrade e a de avisos nos casos que não têm erro nenhum. O que sobra no lugar deles é o mundo atrás da tela, que só vale dentro de
    `LIMITE_MUNDO_POR_LUZ`: se o port desenhar uma aba ali, aparece como `alem_mundo_atras`."""
    deles = (visivel or {}).get(c["nome"])
    if deles is not None:
        ausentes = [nome for nome in ("aba_avisos", "aba_xp") if not deles.get(nome, False)]
        # O Heat Generator não tem a aba de xp: não está nos elementos dele, e o log também não a lista.
        return ausentes
    ausentes = [] if cena_telas.xp_visivel(c) else ["aba_xp"]
    if "_xp_" in c["nome"] or "_nome_" in c["nome"] or "_aba_energia" in c["nome"]:
        ausentes.append("aba_avisos")
    return ausentes


def prioridade_do_caso(c, regioes, janela):
    """O que conta antes dos elementos da tela: o tooltip (que cobre o que estiver embaixo, e sai com a tolerância do fundo de 94 % de opacidade),
    com os cantos do de item à frente, e depois a janela de upgrades."""
    prioridade = {}
    caixa = regioes.get("tooltip")
    if caixa and c["nome"] in CASOS_COM_TOOLTIP_DE_ITEM:
        x, y, w, h = caixa
        k = CANTO_TOOLTIP
        for i, (cx, cy) in enumerate(((x, y), (x + w - k, y), (x, y + h - k), (x + w - k, y + h - k))):
            prioridade[f"tooltip_canto_{i}"] = (cx, cy, k, k)
    if caixa:
        prioridade["tooltip"] = caixa
    tela = cena_telas.MAQUINAS[c["maquina"]]["tela"]
    for nome, rect in (janela or {}).items():
        if nome == "janela_fundo" and tela == "generator":
            # A barra de armadura do Heat Generator (os fundos vazios, os cinco quadros e o suporte) passa por baixo da sombra da janela: o que o original desenha ali é
            # dela, não do mundo atrás da janela. As filhas da janela, e o painel dela, vêm antes.
            barra = ct.elementos_da_tela("generator")
            prioridade.update({n: barra[n] for n in (*ct.FUNDOS_ARMADURA, "slots_armadura", "suporte_armadura")})
        prioridade[nome] = rect
    return prioridade


# Os resíduos com teto: só vale como resíduo a diferença que o lote mediu nesses elementos (docs/plano-paridade.md, "Tetos dos resíduos"). Por escala,
# `(pixels, canal)`: os pixels diferentes no elemento e a maior diferença por canal entre eles (`maior`, de `comparar_tela.comparar`). Passar de um dos
# dois é diferença de verdade: um ícone trocado na lista de upgrades, um canto de tooltip de outra cor, a barra de químico com outro nível ou outra cor.
# A barra e o medidor de lava medem o print do original contra o quadro do ciclo do port que `casar_fases` (ou `fase_do_quimico`) escolheu.
# - Barra de químico (`barra_quimico_fase`): a mistura H26 do sprite dá no máximo 1 nível por canal. Pixels, o pior print de cada ciclo do original contra o quadro
#   mais próximo do port: guiScale 2 carbono 0, fungi 60, bio 104, bio 800 108, bio 1000 192; guiScale 3 carbono 0, fungi 99, bio 162, bio 800 270, bio 1000 342. Os
#   casos sem ciclo (os 17 do Infuser com tanque) deram de 0 a 54 pixels e 1 nível. Teto: o pior de todos, 192 e 342, com 1 nível.
# - Medidor de lava (`medidor_lava_fase`): 0 pixel e 0 nível em todos os quadros e casos (os ciclos a 500 e a 1000 mB, com e sem o cursor, e os casos de medidor
#   parado nas duas escalas): o `lava_still` não tem mistura. Teto 0 e 0.
# - O ícone de cada linha da lista de upgrades é idêntico em guiScale 2 (0 pixel nos casos com a janela): é ali que a troca de ícone aparece. Em guiScale 3 o item
#   em escala 0,75 (H25) cai entre pixels, e o pior do lote é 107 de 576 pixels, 206 por canal; só vale se o mesmo estado deu 0 em guiScale 2 (`residuos_e_reais`).
# - Os cantos do tooltip de item (H23): 6 de 64 pixels e 72 por canal em guiScale 2, 21 de 144 e 72 em 3 (`chamber_hover_slot_item`).
TETO_DO_RESIDUO = {
    "janela_lista_icone": {2: (0, 0), 3: (107, 206)},
    "tooltip_canto": {2: (6, 72), 3: (21, 72)},
    "barra_quimico_fase": {2: (192, 1), 3: (342, 1)},
    "medidor_lava_fase": {2: (0, 0), 3: (0, 0)},
}


def com_particulas(c):
    """Se o print de `c` pode ter as partículas da máquina acesa (`CAIXA_DAS_PARTICULAS`): a máquina acesa no estado, ou a do caso anterior. O original roda os
    casos em sequência no mesmo lugar, e a fumaça e a poeira de redstone que a máquina do caso anterior soltou ainda estão no ar (alguns segundos) quando o
    próximo caso, já com outra máquina e parada, tira o print: `crusher_aba_energia`, `smelter_aba_energia` e `generator_aba_energia` vêm logo depois de um caso aceso."""
    todos = cena_telas.todos_os_casos()
    nomes = [x["nome"] for x in todos]
    anterior = todos[nomes.index(c["nome"]) - 1] if c["nome"] in nomes and nomes.index(c["nome"]) > 0 else None
    return cena_telas.maquina_ativa(c) or (anterior is not None and cena_telas.maquina_ativa(anterior))


def residuo(nome, escala=2, diferentes=0, maior=0):
    """Se a diferença de `diferentes` pixels, a maior delas `maior` por canal, no elemento `nome` é resíduo do Horizon. Sem teto, o nome basta."""
    if not (nome.startswith(PREFIXOS_RESIDUO) or nome in RESIDUOS_DO_HORIZON):
        return False
    teto = next((por_escala[escala] for prefixo, por_escala in TETO_DO_RESIDUO.items() if nome.startswith(prefixo)), None)
    return teto is None or (diferentes <= teto[0] and maior <= teto[1])


def residuos_e_reais(dados, escala, icone_provado=True):
    """Separa os elementos com diferença (`{nome: {"diferentes", "maior"}}`) em resíduos e reais, cada um `{nome: pixels}`. Em guiScale 3 o ícone de
    uma linha da lista cai entre pixels (H25) e o teto dele é largo: só vale como resíduo se o mesmo estado, em guiScale 2, deu o ícone idêntico
    (`icone_provado`), o que prova que o ícone é o certo."""
    eh = {n: residuo(n, escala, d["diferentes"], d["maior"]) and (icone_provado or not n.startswith(ICONE_DA_LISTA)) for n, d in dados.items()}
    return {n: d["diferentes"] for n, d in dados.items() if eh[n]}, {n: d["diferentes"] for n, d in dados.items() if not eh[n]}


def nao_explicadas_de(residuos):
    """Do que passou por resíduo, o que é o mundo atrás da tela (`mundo_atras`, a sombra da janela e os cantos transparentes, que o deixam passar): a
    iluminação dele sai mais clara no original (H30) sem que se tenha achado a causa, e o escurecimento do Horizon só chega a 1 nível do original (H22). O
    limite por luz o segura, mas sem causa provada a diferença não é "ok": o relatório a marca `NE`."""
    return {n: px for n, px in residuos.items() if ct.do_mundo(n)}


def marca_do_estado(reais, nao_explicadas):
    """`DIF` com diferença de verdade, `NE` com só o mundo atrás da tela diferente e sem causa provada, `ok` sem nenhuma."""
    return "DIF" if reais else ("NE " if nao_explicadas else "ok ")


def texto_das_nao_explicadas(nao_explicadas, resultado):
    if not nao_explicadas:
        return ""
    return (f" | NÃO EXPLICADA (mundo atrás da tela, H30 e H22): {nao_explicadas}, até {resultado['maior_do_mundo']} por canal; sem causa provada, não é ok")


def conhecidas_de(dados):
    """Tira de `dados` as diferenças conhecidas (`DIFERENCAS_CONHECIDAS`: o que difere por construção) e as devolve, `{nome: pixels}`. Elas não são resíduo
    do Horizon, e não ficam caladas: o relatório as conta e diz a causa."""
    return {n: dados.pop(n)["diferentes"] for n in list(dados) if n in DIFERENCAS_CONHECIDAS}


def texto_das_conhecidas(conhecidas):
    return " | CONHECIDA, sorteada por cada jogo: " + "; ".join(f"{n} {px} px ({DIFERENCAS_CONHECIDAS[n]})" for n, px in conhecidas.items()) if conhecidas else ""


# O sprite do químico é uma tira animada, e o original e o port contam a animação por conta própria (o original por tick do cliente, o port por
# passo do servidor): um print único cai numa fase que o outro jogo não escolheu. A prova de que a diferença é só fase, e não nível, posição ou
# textura, usa o ciclo inteiro do mesmo químico com o mesmo tanque (`CICLO_DO_QUIMICO`, casos que andam um passo por print): o print do port tem de ser,
# pixel a pixel, um quadro do ciclo do port (o port é uma fase da própria animação), e o do original tem de estar, de um quadro do ciclo do port,
# a no máximo o pior custo do ciclo (`casar_fases`: a mistura H26 do sprite, que o original faz na GPU). A barra com o tanque pela metade não é um recorte
# da cheia (o sprite se repete a partir do topo do líquido), por isso a referência é do mesmo nível.
CICLO_DO_QUIMICO = {
    ("bio", 500): "infuser_tanque_bio_ciclo",
    ("bio", 800): "infuser_fase_bio_800",
    ("bio", 1000): "infuser_fase_bio_1000",
    ("fungi", 500): "infuser_tanque_fungi_ciclo",
    ("carbon", 500): "infuser_tanque_carbono",
    ("redstone", 9): "infuser_fase_redstone_9",
    ("redstone", 10): "infuser_fase_redstone_10",
}


# A lava do medidor do Heat Generator é a mesma tira animada (`lava_still`, 38 quadros de 2 ticks), e o medidor de um caso sem ciclo próprio se explica
# pelo ciclo do mesmo nível: `(lava em mB, cursor sobre o medidor)`. Com o cursor em cima, o medidor leva o realce e o tooltip, e só o ciclo com o cursor
# ali (`generator_fase_lava_hover`) tem quadros iguais ao print.
CICLO_DA_LAVA = {
    (500, False): "generator_lava_ciclo",
    (1000, False): "generator_fase_lava_1000",
    (500, True): "generator_fase_lava_hover",
}


def ciclo_do_caso(c):
    """O caso de ciclo que serve de referência ao tanque de `c` (o de químico do Infuser, ou o medidor de lava do Heat Generator), ou `None`."""
    if "lava" in c["args"]:
        sobre_o_medidor = c["args"].get("hover") == cena_telas.HOVER["generator"]["medidor_lava"]
        return CICLO_DA_LAVA.get((int(c["args"]["lava"]), sobre_o_medidor))
    quimico, _, quantidade = c["args"].get("quimico", "").partition(":")
    return CICLO_DO_QUIMICO.get((quimico, int(quantidade or 0)))


def animado_da_tela(tela):
    """O elemento animado de cada tela: a barra de químico do Infuser, o medidor de lava do Heat Generator."""
    return "barra_quimico" if tela == "infuser" else "medidor_lava"


def maior_diferenca(a, b):
    """A maior diferença por canal entre dois recortes."""
    return max((abs(ra[i + k] - rb[i + k]) for ra, rb in zip(a, b) for i in range(0, len(ra), 4) for k in range(3)), default=0)


def fase_do_quimico(c, escala, original, port):
    """Se o `barra_quimico` (ou o `medidor_lava`) do caso `c` (um tanque animado, sem ciclo próprio) é só fase da animação. Devolve `(ok, texto, medida)`, com `medida` a
    diferença do original para o quadro mais próximo do ciclo de referência (`{"diferentes", "maior"}`, `None` sem ciclo). Vale o teto absoluto do resíduo
    (`TETO_DO_RESIDUO["barra_quimico_fase"]`, medido no lote), não o pior custo do próprio ciclo: um ciclo errado não alarga o que passa."""
    nome_ciclo = ciclo_do_caso(c)
    ciclo = next((x for x in cena_telas.todos_os_casos() if x["nome"] == nome_ciclo), None)
    if ciclo is None:
        return False, "sem ciclo de referência", None
    tela = cena_telas.MAQUINAS[c["maquina"]]["tela"]
    animado = animado_da_tela(tela)
    rect = ct.elementos_da_tela(tela)[animado]
    nome = f"{c['nome']}_e{escala}"
    o = recorte(ct.ler_png(os.path.join(original, f"oficial_{nome}.png")), escala, rect)
    p = recorte(ct.ler_png(os.path.join(port, f"port_{nome}.png")), escala, rect)
    m = cena_telas.ciclos(ciclo, "port")
    quadros = [recorte(ct.ler_png(os.path.join(port, f"port_{ciclo['nome']}_e{escala}_{k}.png")), escala, rect) for k in range(m)]
    melhor_port = min(diferentes(p, q) for q in quadros)
    melhor_orig, k = min((diferentes(o, q), k) for k, q in enumerate(quadros))
    medida = {"diferentes": melhor_orig, "maior": maior_diferenca(o, quadros[k])}
    teto = TETO_DO_RESIDUO[animado + "_fase"][escala]
    ok = melhor_port == 0 and residuo(animado + "_fase", escala, **medida)
    return ok, (f"port {melhor_port} px do quadro mais próximo do ciclo {nome_ciclo}, original {melhor_orig} px e {medida['maior']} por canal "
                f"(teto {teto[0]} px e {teto[1]} por canal)"), medida


def principal(argv):
    so = None
    if "--so" in argv:
        i = argv.index("--so")
        so = argv[i + 1].split(",")
        del argv[i : i + 2]
    original, port, saida = argv[:3]
    escalas = [int(e) for e in argv[3:]]
    os.makedirs(saida, exist_ok=True)
    visivel = ler_visibilidade(os.path.join(original, "oficial.log"))
    mundo_max = 0
    casos = [c for c in cena_telas.todos_os_casos() if (not so or c["nome"] in so) and "so_oficial" not in c["args"]]
    resultados = []
    falhas = 0
    realmente = 0  # estados com diferença fora dos resíduos
    com_conhecidas = 0  # estados com uma diferença conhecida (`DIFERENCAS_CONHECIDAS`)
    com_nao_explicadas = 0  # estados sem diferença de verdade, só com o mundo atrás da tela diferente e sem causa provada (`NE`)
    icone_em_2 = {}  # por (caso, captura): os pixels de ícone da lista que o estado deu em guiScale 2
    faltam = 0  # estados sem print de um dos lados
    for escala in escalas:
        for c in casos:
            if "ciclo" not in c["args"]:
                continue
            try:
                casados, pa, pb = casar_fases(c, escala, original, port)
            except FileNotFoundError:
                print(f"FALTA {c['nome']}_e{escala} (ciclo)")
                falhas += 1
                faltam += 1
                continue
            n = cena_telas.ciclos(c, "oficial")
            distintos = len({tuple(x) for x in casados})
            sem_par = sum(1 for custo, _ in casados if custo)
            print(f"ciclo {c['nome']}_e{escala}: {len(casados) - sem_par} de {len(casados)} prints do original têm um estado idêntico no port; {sem_par} sem par (pior {max(custo for custo, _ in casados)} px)")
            tela = cena_telas.MAQUINAS[c["maquina"]]["tela"]
            for k in range(n):
                a, b = ct.ler_png(pa[k]), ct.ler_png(pb[casados[k][1]])
                # O cursor de um ciclo com `hover` (o medidor de lava com o tooltip) cobre o que está embaixo, como nos casos sem ciclo.
                regioes = regioes_do_caso(c, a, b, escala)
                resultado, diferenca = ct.comparar(a, b, tela, escala, regioes=regioes, prioridade=prioridade_do_caso(c, regioes, None), mascarar_abas=False,
                                                   limite_mundo=LIMITE_MUNDO_POR_LUZ[cena_telas.luz_do_caso(c)])
                mundo_max = max(mundo_max, resultado["maior_do_mundo"])
                resultado["caso"] = f"{c['nome']}_e{escala}_{k}"
                resultados.append(resultado)
                falhas += resultado["diferentes"] > 0
                # O sprite animado de um ciclo é a medida da fase e da mistura H26 (a linha `ciclo` acima): ele é o resíduo, o resto da tela não.
                dados = {n_: d_ for n_, d_ in resultado["elementos"].items() if d_["diferentes"]}
                animado = "barra_quimico" if tela == "infuser" else "medidor_lava"
                if animado in dados:
                    dados[animado + "_fase"] = dados.pop(animado)
                conhecidas = conhecidas_de(dados)
                residuos, reais = residuos_e_reais(dados, escala)
                realmente += bool(reais)
                com_conhecidas += bool(conhecidas)
                if k == 0 or dados:
                    sufixo = "" if escala == 2 else f"_e{escala}"
                    ct.gravar_png(os.path.join(saida, f"tela_{c['nome']}_{k}{sufixo}.png"), *ct.lado_a_lado(a, b, diferenca, resultado["recorte"]))
                nao_explicadas = nao_explicadas_de(residuos)
                com_nao_explicadas += bool(nao_explicadas) and not reais
                if reais or conhecidas or k == 0:
                    print(f"  {marca_do_estado(reais, nao_explicadas)} {resultado['caso']:<44} {resultado['diferentes']:>6} fora dos resíduos do Horizon: {reais if reais else 0}"
                          + (f" | resíduos: {residuos}" if residuos else "") + texto_das_conhecidas(conhecidas)
                          + texto_das_nao_explicadas(nao_explicadas, resultado), flush=True)
        for c, k, nome, caminho_a, caminho_b in pares([x for x in casos if "ciclo" not in x["args"]], escala, original, port):
            if not (os.path.exists(caminho_a) and os.path.exists(caminho_b)):
                print(f"FALTA {nome}")
                falhas += 1
                faltam += 1
                continue
            tela = cena_telas.MAQUINAS[c["maquina"]]["tela"]
            a, b = ct.ler_png(caminho_a), ct.ler_png(caminho_b)
            # Com a janela aberta, os filhos dela têm prioridade e o recorte inclui a janela e a aba.
            janela = ct.JANELA if "clicar" in c["args"] and "%d,%d" % cena_telas.ABA in c["args"]["clicar"].split(";") else None
            regioes = regioes_do_caso(c, a, b, escala)
            resultado, diferenca = ct.comparar(
                a, b, tela, escala, regioes=regioes, mascarar_abas=False,
                margem=28 if "_aba_" in c["nome"] or "_xp_" in c["nome"] else 0, prioridade=prioridade_do_caso(c, regioes, janela),
                ausentes=ausentes_do_caso(c, visivel), limite_mundo=LIMITE_MUNDO_POR_LUZ[cena_telas.luz_do_caso(c)],
                particulas=CAIXA_DAS_PARTICULAS if com_particulas(c) else None,
            )
            resultado["caso"] = nome
            mundo_max = max(mundo_max, resultado["maior_do_mundo"])
            dados = {n: d for n, d in resultado["elementos"].items() if d["diferentes"]}
            if animado_da_tela(tela) in dados and ("quimico" in c["args"] or "lava" in c["args"]):
                try:
                    ok, texto, medida = fase_do_quimico(c, escala, original, port)
                except FileNotFoundError as erro:
                    ok, texto, medida = False, f"falta {erro.filename}", None
                resultado["fase_do_quimico"] = {"ok": ok, "texto": texto}
                print(f"   fase do químico em {nome}: {'ok' if ok else 'NÃO FECHA'}: {texto}")
                if ok:
                    # A fase é a diferença do original para o quadro do ciclo, não a do print do port (outra fase da mesma animação).
                    dados.pop(animado_da_tela(tela))
                    dados[animado_da_tela(tela) + "_fase"] = medida
            resultados.append(resultado)
            ruim = resultado["diferentes"] > 0
            falhas += ruim
            sufixo = "" if escala == 2 else f"_e{escala}"
            extra = f"_{k}" if int(c["args"].get("ciclo", "0")) or int(c["args"].get("capturas", "1")) > 1 else ""
            destino = os.path.join(saida, f"tela_{c['nome']}{extra}{sufixo}.png")
            ct.gravar_png(destino, *ct.lado_a_lado(a, b, diferenca, resultado["recorte"]))
            conhecidas = conhecidas_de(dados)
            if escala == 2:
                icone_em_2[(c["nome"], k)] = sum(d["diferentes"] for n_, d in dados.items() if n_.startswith(ICONE_DA_LISTA))
            residuos, reais = residuos_e_reais(dados, escala, icone_em_2.get((c["nome"], k)) == 0)
            com_conhecidas += bool(conhecidas)
            nao_explicadas = nao_explicadas_de(residuos)
            com_nao_explicadas += bool(nao_explicadas) and not reais
            print(f"{marca_do_estado(reais, nao_explicadas)} {nome:<44} {resultado['diferentes']:>6} fora dos resíduos do Horizon: {reais if reais else 0}"
                  + (f" | resíduos: {residuos}" if residuos else "") + texto_das_conhecidas(conhecidas)
                          + texto_das_nao_explicadas(nao_explicadas, resultado), flush=True)
            if reais:
                realmente += 1
                for amostra in resultado["alem_amostra"][:3]:
                    print(f"     além do limite do mundo: {amostra}")
    with open(os.path.join(saida, "resultado.json"), "w") as arquivo:
        json.dump(resultados, arquivo, indent=1, ensure_ascii=False)
    total = sum(r["diferentes"] for r in resultados)
    print(f"{len(resultados)} estados, {faltam} sem print, {realmente} com diferença fora dos resíduos, {com_conhecidas} com diferença conhecida (sorteada por cada jogo), {com_nao_explicadas} só com o mundo atrás da tela diferente e sem causa provada (NE), "
          f"{falhas - faltam} com algum pixel diferente, {total} pixels diferentes no total; o mundo atrás da tela difere até {mundo_max} por canal "
          f"(limite por luz {LIMITE_MUNDO_POR_LUZ})")
    return 1 if realmente or faltam else 0


if __name__ == "__main__":
    sys.exit(principal(sys.argv[1:]))
