#!/usr/bin/env python3
"""Compara o Energy Cube no mundo e como item entre o original e o port: o roteiro `cenas/cubo.json` (`oficial_blocos.sh` no original, `ROTEIRO=cubo.json h2_blocos.py cena` no port).

    comparar_cubo.py PASTA_DO_ORIGINAL PASTA_DO_PORT PASTA_DE_SAIDA

Os tiros `cubo_*` mostram os cinco cubos lado a lado (a faixa onde ficam, como `comparar_blocos.py`), os `item_*` a barra de atalhos (o item na interface) e a mão (o item
em primeira pessoa). Por tiro e região: os pixels que diferem (qualquer canal) e o maior desvio por canal; grava `cubo_<tiro>.png` com o original, o port e a diferença.
Código de saída 1 se alguma região tem diferença.
"""
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import comparar_tela as ct

LARGURA, ALTURA = 1920, 1080
# Em pixels da imagem de 1920 por 1080 (guiScale 4): onde cada coisa fica.
REGIOES = {
    "mundo": (440, 650, 500, 1420),
    "hotbar": (990, 1080, 590, 1330),
    "mao": (560, 1080, 1150, 1920),
}


def regioes_do_tiro(nome):
    return ("mundo",) if nome.startswith("cubo_") else ("hotbar", "mao")


def terra(a, b, i):
    """Terra nua (vermelho acima do verde acima do azul) em qualquer um dos dois: a grama sob os blocos morre ao acaso, e o mundo de cada jogo tem as suas manchas."""
    return any(img[i] > img[i + 1] + 12 and img[i + 1] > img[i + 2] + 12 for img in (a, b))


# O mundo de cada jogo tem o seu tom de grama (também atrás da barra de atalhos e da mão) (o bioma e o céu) e a luz do chão é a do Horizon (H30): na região do mundo só conta o desvio acima deste limite por canal.
LIMITE = {"mundo": int(os.environ.get("LIMITE_MUNDO", "24")), "hotbar": int(os.environ.get("LIMITE_MUNDO", "24")), "mao": int(os.environ.get("LIMITE_MUNDO", "24"))}


def comparar_regiao(a, b, regiao, usa_terra):
    y0, y1, x0, x1 = REGIOES[regiao]
    diferentes = maior = 0
    caixa = [10**9, 10**9, -1, -1]
    saida = bytearray(b"\x00\x00\x00\xff" * ((y1 - y0) * (x1 - x0)))
    for y in range(y0, y1):
        for x in range(x0, x1):
            i = (y * LARGURA + x) * 4
            d = max(abs(a[i + k] - b[i + k]) for k in range(3))
            if d > LIMITE[regiao] and not (usa_terra and terra(a, b, i)):
                diferentes += 1
                maior = max(maior, d)
                caixa = [min(caixa[0], x), min(caixa[1], y), max(caixa[2], x), max(caixa[3], y)]
                j = ((y - y0) * (x1 - x0) + (x - x0)) * 4
                saida[j : j + 4] = bytes((255, 0, 0, 255))
    return diferentes, maior, caixa, saida


def tres_em_coluna(recortes, largura):
    altura = sum(len(r) // (largura * 4) for r in recortes)
    return largura, altura, b"".join(bytes(r) for r in recortes)


def principal(argv):
    original, port, saida = argv[:3]
    os.makedirs(saida, exist_ok=True)
    nomes = sorted(f[len("oficial_"):-4] for f in os.listdir(original) if f.startswith("oficial_") and f.endswith(".png") and f[len("oficial_"):-4].startswith(("cubo_", "item_")))
    falhas = 0
    for nome in nomes:
        caminho_porte = os.path.join(port, f"porte_{nome}.png")
        if not os.path.exists(caminho_porte):
            print(f"FALTA {nome}")
            falhas += 1
            continue
        _, _, a = ct.ler_png(os.path.join(original, f"oficial_{nome}.png"))
        _, _, b = ct.ler_png(caminho_porte)
        for regiao in regioes_do_tiro(nome):
            y0, y1, x0, x1 = REGIOES[regiao]
            diferentes, maior, caixa, mapa = comparar_regiao(a, b, regiao, usa_terra=regiao == "mundo")
            falhas += diferentes > 0
            print(f"{'DIF' if diferentes else 'ok '} {nome:<22} {regiao:<7} {diferentes:>7} px, até {maior} por canal, caixa {caixa if diferentes else '-'}")
            largura = x1 - x0
            corte = lambda img: b"".join(bytes(img[((y * LARGURA) + x0) * 4 : ((y * LARGURA) + x1) * 4]) for y in range(y0, y1))
            ct.gravar_png(os.path.join(saida, f"cubo_{nome}_{regiao}.png"), *tres_em_coluna([corte(a), corte(b), mapa], largura))
    return 1 if falhas else 0


if __name__ == "__main__":
    sys.exit(principal(sys.argv[1:]))
