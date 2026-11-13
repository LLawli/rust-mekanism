#!/usr/bin/env python3
"""Empilha o recorte do original sobre o recorte do port, no mesmo trecho, ampliado, e conta os pixels diferentes.

    empilhar_recortes.py ORIGINAL.png PORT.png --regiao X,Y,L,A [--escala N] [--tolerancia N] --saida EMPILHADO.png

Os dois prints têm o mesmo tamanho (1920 por 1080). O original fica em cima e o port embaixo, separados por uma faixa; cada pixel vira um quadrado de `escala` por `escala`
(padrão 4). Escreve a contagem de pixels diferentes do trecho (com a tolerância por canal, padrão 0). O código de saída é 0 sem diferença, 1 com alguma, 2 para entrada inválida.
Só a biblioteca padrão: usa o leitor de PNG do `comparar_tela` e a conta do `comparar_efeitos`.
"""
import argparse
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from comparar_efeitos import comparar  # noqa: E402
from comparar_tela import gravar_png, ler_png  # noqa: E402

FAIXA = 6


def recortar(imagem, regiao, escala):
    """O trecho `regiao` (x, y, largura, altura) de `(largura, altura, rgba)`, com cada pixel repetido `escala` vezes nos dois eixos. Devolve (largura, altura, rgba)."""
    larg_total, _, pixels = imagem
    x0, y0, larg, alt = regiao
    saida = bytearray()
    for y in range(alt):
        linha = bytearray()
        for x in range(larg):
            i = ((y0 + y) * larg_total + x0 + x) * 4
            linha += bytes(pixels[i : i + 4]) * escala
        saida += bytes(linha) * escala
    return larg * escala, alt * escala, bytes(saida)


def empilhar(original, port, regiao, escala=4):
    """Original em cima, port embaixo e uma faixa cinza entre os dois."""
    cima = recortar(original, regiao, escala)
    baixo = recortar(port, regiao, escala)
    larg = cima[0]
    faixa = bytes((128, 128, 128, 255)) * (larg * FAIXA)
    return larg, cima[1] + FAIXA + baixo[1], cima[2] + faixa + baixo[2]


def principal(argv=None):
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("original")
    parser.add_argument("port")
    parser.add_argument("--regiao", required=True, help="X,Y,L,A em pixels da imagem")
    parser.add_argument("--escala", type=int, default=4)
    parser.add_argument("--tolerancia", type=int, default=0)
    parser.add_argument("--saida", required=True)
    args = parser.parse_args(argv)
    try:
        regiao = tuple(int(v) for v in args.regiao.split(","))
        if len(regiao) != 4 or args.escala < 1:
            raise ValueError("região de quatro números e escala de 1 ou mais")
        original, port = ler_png(args.original), ler_png(args.port)
        diferentes, total, _, _ = comparar(original, port, regiao, args.tolerancia)
        larg, alt, pixels = empilhar(original, port, regiao, args.escala)
    except (ValueError, OSError) as erro:
        print(f"erro: {erro}", file=sys.stderr)
        return 2
    gravar_png(args.saida, larg, alt, pixels)
    print(f"{diferentes} de {total} pixels diferentes ({100 * diferentes / total:.4f} %)")
    return 1 if diferentes else 0


if __name__ == "__main__":
    sys.exit(principal())
