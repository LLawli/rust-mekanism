#!/usr/bin/env python3
"""Compara o medidor de lava do Heat Generator nos prints do original (`oficial_lava_<nivel>.png`) e do port (`porte_lava_<nivel>.png`, `cenas/heat_lava.json`): a silhueta da lava
(o desenho do tanque e o que passa do medidor até o topo da tela) pixel a pixel, e a textura inteira.

    comparar_lava.py PASTA_DO_ORIGINAL PASTA_DO_PORT SAIDA

A textura da lava é animada (a fase do quadro depende do tick de cada jogo), então a textura pixel a pixel nunca fecha em zero entre dois jogos sem o tempo igualado; a extensão (a primeira e a última linha de lava de cada coluna) fecha em zero quando a lava ocupa o mesmo lugar, e a silhueta pixel a pixel só difere nos pixels escuros da textura. Para cada nível escreve as duas contagens, a linha mais alta de lava de cada jogo e o recorte
empilhado (o port em cima, o original embaixo) em `SAIDA/empilhado_<nivel>.png`. Só a biblioteca padrão.
"""
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from comparar_efeitos import comparar  # noqa: E402
from comparar_item import REGIOES, regiao_de  # noqa: E402
from comparar_tela import gravar_png, ler_png  # noqa: E402
from empilhar_recortes import empilhar  # noqa: E402


def e_lava(r, g, b):
    """Laranja saturado da textura da lava (os pixels do fundo do mundo e da tela cinza ficam de fora)."""
    return r >= 150 and b <= 90 and r - b >= 100


def silhueta(imagem, regiao):
    largura, _, pixels = imagem
    x0, y0, larg, alt = regiao
    mascara = bytearray(larg * alt)
    for y in range(alt):
        for x in range(larg):
            i = ((y0 + y) * largura + x0 + x) * 4
            if e_lava(pixels[i], pixels[i + 1], pixels[i + 2]):
                mascara[y * larg + x] = 1
    return mascara


def extensao(mascara, larg):
    """Por coluna, a primeira e a última linha de lava: a forma da lava sem a textura, que a fase da animação muda pixel a pixel."""
    colunas = []
    for x in range(larg):
        linhas = [i // larg for i in range(x, len(mascara), larg) if mascara[i]]
        colunas.append((linhas[0], linhas[-1]) if linhas else None)
    return colunas


def topo(mascara, larg):
    for indice, marcado in enumerate(mascara):
        if marcado:
            return indice // larg
    return None


def principal(argv=None):
    pasta_original, pasta_port, saida = (argv or sys.argv[1:])[:3]
    os.makedirs(saida, exist_ok=True)
    niveis = sorted(f[len("oficial_lava_") : -4] for f in os.listdir(pasta_original) if f.startswith("oficial_lava_") and f.endswith(".png") and "fim" not in f)
    total = 0
    for nivel in niveis:
        original = ler_png(os.path.join(pasta_original, f"oficial_lava_{nivel}.png"))
        port = ler_png(os.path.join(pasta_port, f"porte_lava_{nivel}.png"))
        regiao = regiao_de(f"lava_{nivel}")
        larg = regiao[2]
        mo, mp = silhueta(original, regiao), silhueta(port, regiao)
        diferentes = sum(1 for a, b in zip(mo, mp) if a != b)
        textura = comparar(original, port, regiao, 0)[0]
        l, a, rgba = empilhar(port, original, regiao, 2)
        gravar_png(os.path.join(saida, f"empilhado_lava_{nivel}.png"), l, a, rgba)
        colunas = sum(1 for a, b in zip(extensao(mo, larg), extensao(mp, larg)) if a != b)
        print(f"lava_{nivel}: extensão da lava, {colunas} de {larg} colunas diferentes; silhueta {diferentes} pixels diferentes (lava: original {sum(mo)}, port {sum(mp)}; topo da lava no recorte: original {topo(mo, larg)}, port {topo(mp, larg)}); textura {textura} de {len(mo)}")
        total += colunas
    print(f"{len(niveis)} níveis, {total} colunas de extensão diferentes")
    return 0 if total == 0 else 1


if __name__ == "__main__":
    sys.exit(principal())
