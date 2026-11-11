#!/usr/bin/env python3
"""Compara dois prints dos efeitos de tela do Configurator (original e port), pixel a pixel.

    comparar_efeitos.py ORIGINAL.png PORT.png [--regiao X,Y,L,A] [--tolerancia N] [--saida LADO_A_LADO.png]

Os dois prints têm o mesmo tamanho (1920 por 1080). Sem `--regiao` compara a imagem inteira; com ela, só o retângulo (em pixels da imagem), e o lado a lado sai recortado.
`--tolerancia` é a diferença máxima por canal que ainda conta como igual (padrão 0). Escreve a quantidade e a fração de pixels diferentes e, com `--saida`, o original, o port e a
diferença (vermelho onde difere) lado a lado. O código de saída é 0 sem diferença, 1 com alguma, 2 para entrada inválida. Só a biblioteca padrão: usa o leitor de PNG do `comparar_tela`.
"""
import argparse
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from comparar_tela import gravar_png, ler_png  # noqa: E402


def comparar(original, port, regiao=None, tolerancia=0):
    """Devolve (diferentes, total, mascara, recorte) dos dois (largura, altura, rgba)."""
    (l1, a1, p1), (l2, a2, p2) = original, port
    if (l1, a1) != (l2, a2):
        raise ValueError(f"tamanhos diferentes: {l1}x{a1} e {l2}x{a2}")
    x0, y0, larg, alt = regiao or (0, 0, l1, a1)
    if x0 < 0 or y0 < 0 or x0 + larg > l1 or y0 + alt > a1:
        raise ValueError("a região sai da imagem")
    diferentes = 0
    mascara = bytearray(larg * alt)
    for y in range(alt):
        inicio = ((y0 + y) * l1 + x0) * 4
        a, b = p1[inicio : inicio + larg * 4], p2[inicio : inicio + larg * 4]
        if tolerancia == 0 and a == b:
            continue
        for x in range(larg):
            i = x * 4
            if max(abs(a[i] - b[i]), abs(a[i + 1] - b[i + 1]), abs(a[i + 2] - b[i + 2])) > tolerancia:
                mascara[y * larg + x] = 1
                diferentes += 1
    return diferentes, larg * alt, mascara, (x0, y0, larg, alt)


def lado_a_lado(original, port, mascara, recorte, caminho):
    l1, _, p1 = original
    _, _, p2 = port
    x0, y0, larg, alt = recorte
    saida = bytearray(larg * 3 * alt * 4)
    for y in range(alt):
        base = ((y0 + y) * l1 + x0) * 4
        for k, fonte in enumerate((p1, p2)):
            destino = (y * larg * 3 + k * larg) * 4
            saida[destino : destino + larg * 4] = fonte[base : base + larg * 4]
        destino = (y * larg * 3 + 2 * larg) * 4
        for x in range(larg):
            i = destino + x * 4
            if mascara[y * larg + x]:
                saida[i : i + 4] = b"\xff\x00\x00\xff"
            else:
                saida[i : i + 3] = bytes(c // 3 for c in p1[base + x * 4 : base + x * 4 + 3])
                saida[i + 3] = 255
    gravar_png(caminho, larg * 3, alt, bytes(saida))


def principal(argv=None):
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("original")
    parser.add_argument("port")
    parser.add_argument("--regiao", help="X,Y,L,A em pixels da imagem")
    parser.add_argument("--tolerancia", type=int, default=0)
    parser.add_argument("--saida")
    args = parser.parse_args(argv)
    try:
        regiao = tuple(int(v) for v in args.regiao.split(",")) if args.regiao else None
        original, port = ler_png(args.original), ler_png(args.port)
        diferentes, total, mascara, recorte = comparar(original, port, regiao, args.tolerancia)
    except (ValueError, OSError) as erro:
        print(f"erro: {erro}", file=sys.stderr)
        return 2
    print(f"{diferentes} de {total} pixels diferentes ({100 * diferentes / total:.4f} %)")
    if args.saida:
        lado_a_lado(original, port, mascara, recorte, args.saida)
    return 1 if diferentes else 0


if __name__ == "__main__":
    sys.exit(principal())
