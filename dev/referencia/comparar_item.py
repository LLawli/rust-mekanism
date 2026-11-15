#!/usr/bin/env python3
"""Compara, tiro a tiro, os prints do original (`oficial_<tiro>.png`, `oficial_item.sh`) e os do port (`porte_<tiro>.png`, `h2_blocos.py cena` com o roteiro `item_conteudo.json`) da frente
`item-conteudo`: conta os pixels diferentes de um recorte, diz a caixa em que eles caem e grava o recorte empilhado (o port em cima, o original embaixo) em
`SAIDA/empilhado_<tiro>.png`.

    comparar_item.py PASTA_DO_ORIGINAL PASTA_DO_PORT SAIDA [--escala N] [--tolerancia N]

O recorte de cada tipo de tiro é o de `REGIOES`, pelo prefixo do nome (a tela da máquina com a janela de upgrades, ou o inventário com o tooltip). Só a biblioteca padrão.
"""
import argparse
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from comparar_efeitos import comparar  # noqa: E402
from comparar_tela import gravar_png, ler_png  # noqa: E402
from empilhar_recortes import empilhar  # noqa: E402

# (x, y, largura, altura) em pixels da imagem de 1920 por 1080, com `guiScale` 4. A tela da máquina (176 por 166) fica em (608, 208), e a janela de upgrades abre à esquerda dela; o
# inventário (176 por 166) na mesma posição, com o tooltip ao lado do cursor, sobre o slot 0 da barra.
REGIOES = {
    # o tooltip do slot 0 da barra (o cursor em (674, 810)): a caixa de até 6 linhas ao lado do slot
    "tooltip": (705, 745, 695, 150),
    # o corpo da tela da máquina (176 por 166 em (608, 208)); as abas à direita do original (segurança, cartão e redstone) são de outras frentes e ficam fora
    "tela": (500, 200, 810, 680),
    # a janela de upgrades (abre à esquerda e sobre a tela)
    "aba": (560, 260, 800, 400),
    # o medidor de lava do Heat Generator (frente `maquina-sobras`, `cenas/heat_lava.json`) e o que ele desenha acima de si quando o tanque passa da capacidade, até o topo da tela
    "lava": (780, 0, 360, 500),
    # o tooltip de um item com `DETAILS` (frente `maquina-sobras`, `cenas/detalhes_itens.json`): até 9 linhas, que sobem do cursor (674, 810) quando a caixa passa da altura
    "detalhe": (690, 250, 900, 600),
}


def regiao_de(nome):
    for prefixo, regiao in REGIOES.items():
        if nome.startswith(prefixo):
            return regiao
    return None


def caixa_das_diferencas(mascara, largura, alto, origem):
    """A caixa (x, y, largura, altura), em pixels da imagem, que cobre todo pixel marcado, ou None."""
    xs = [i % largura for i, marcado in enumerate(mascara) if marcado]
    if not xs:
        return None
    ys = [i // largura for i, marcado in enumerate(mascara) if marcado]
    x0, y0 = origem
    return (x0 + min(xs), y0 + min(ys), max(xs) - min(xs) + 1, max(ys) - min(ys) + 1)


def tiros(pasta_original, pasta_port):
    originais = {f[len("oficial_") : -4] for f in os.listdir(pasta_original) if f.startswith("oficial_") and f.endswith(".png")}
    portes = {f[len("porte_") : -4] for f in os.listdir(pasta_port) if f.startswith("porte_") and f.endswith(".png")}
    return sorted(originais & portes), sorted(originais - portes), sorted(portes - originais)


def principal(argv=None):
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("original")
    parser.add_argument("port")
    parser.add_argument("saida")
    parser.add_argument("--escala", type=int, default=2)
    parser.add_argument("--tolerancia", type=int, default=0)
    args = parser.parse_args(argv)
    comuns, so_original, so_port = tiros(args.original, args.port)
    os.makedirs(args.saida, exist_ok=True)
    for nome in so_original:
        print(f"{nome}: só o original tem o print")
    for nome in so_port:
        print(f"{nome}: só o port tem o print")
    total = 0
    for nome in comuns:
        original = ler_png(os.path.join(args.original, f"oficial_{nome}.png"))
        port = ler_png(os.path.join(args.port, f"porte_{nome}.png"))
        regiao = regiao_de(nome)
        if not regiao:
            continue
        no_recorte, area, mascara, _ = comparar(original, port, regiao, args.tolerancia)
        caixa = caixa_das_diferencas(mascara, regiao[2], regiao[3], (regiao[0], regiao[1]))
        larg, alt, rgba = empilhar(port, original, regiao, args.escala)
        gravar_png(os.path.join(args.saida, f"empilhado_{nome}.png"), larg, alt, rgba)
        print(f"{nome}: {no_recorte} de {area} pixels diferentes no recorte {regiao}, caixa {caixa}", flush=True)
        total += no_recorte
    print(f"{len(comuns)} tiros comparados, {total} pixels diferentes no total")
    return 0 if total == 0 else 1


if __name__ == "__main__":
    sys.exit(principal())
