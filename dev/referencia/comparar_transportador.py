#!/usr/bin/env python3
"""Conta os pixels diferentes entre os prints do original e os do port nas cenas do transportador (frente `transportador-sobras`, Fase 4), com `magick compare -metric AE` (sem tolerância) e
empilha os recortes (original em cima, port embaixo) com `empilhar_recortes.py`.

    comparar_transportador.py vidro ORIGINAL_DIR PORT_DIR SAIDA_DIR [--regiao X,Y,L,A] [--passo-x N] [--escala N]
        os 18 vidros: `transportador_vidro_<j>` (três transportadores por tiro, `oficial_` no original e `porte_` no port); a região é a do primeiro transportador do tiro e `passo-x` o
        deslocamento dos outros dois; escreve `vidro_<cor>.png` (o recorte empilhado) e a contagem de cada cor
    comparar_transportador.py passos ORIGINAL_DIR PORT_DIR PREFIXO [--regiao X,Y,L,A]
        a matriz de pixels diferentes entre os tiros `<PREFIXO>_<i>` do original e `<PREFIXO>_<j>` do port (a cada `tick step 1` um tiro), e o melhor par de cada linha
    comparar_transportador.py par ORIGINAL.png PORT.png SAIDA.png [--regiao X,Y,L,A] [--escala N]
        um par só
"""
import argparse
import re
import os
import subprocess
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)

NOMES_DAS_CORES = ["black", "dark_blue", "dark_green", "dark_aqua", "dark_red", "purple", "orange", "gray", "dark_gray", "indigo", "bright_green", "aqua", "red", "pink", "yellow", "white", "brown", "bright_pink"]


def contar(a, b, regiao=None):
    """Os pixels diferentes de dois PNG do mesmo tamanho (`magick compare -metric AE`), na região X,Y,L,A ou na imagem toda."""
    entradas = []
    for caminho in (a, b):
        if regiao:
            x, y, larg, alt = regiao
            entradas.append(f"{caminho}[{larg}x{alt}+{x}+{y}]")
        else:
            entradas.append(caminho)
    saida = subprocess.run(["magick", "compare", "-metric", "AE", *entradas, "null:"], capture_output=True, text=True)
    return round(float((saida.stderr or "0").split()[0]))


def empilhar(a, b, regiao, saida, escala):
    from empilhar_recortes import principal
    return principal([a, b, "--regiao", ",".join(str(v) for v in regiao), "--escala", str(escala), "--saida", saida])


def regiao_de(texto, padrao):
    return tuple(int(v) for v in texto.split(",")) if texto else padrao


def vidro(args):
    base = regiao_de(args.regiao, (570, 470, 180, 140))
    os.makedirs(args.saida, exist_ok=True)
    for cor, nome in enumerate(NOMES_DAS_CORES):
        tiro, k = divmod(cor, 3)
        a = os.path.join(args.original, f"oficial_transportador_vidro_{tiro}.png")
        b = os.path.join(args.port, f"porte_transportador_vidro_{tiro}.png")
        regiao = (base[0] + k * args.passo_x, base[1], base[2], base[3])
        diferentes = contar(a, b, regiao)
        total = regiao[2] * regiao[3]
        saida = os.path.join(args.saida, f"vidro_{cor:02d}_{nome}.png")
        empilhar(a, b, regiao, saida, args.escala)
        print(f"cor {cor:2d} {nome:12s} {diferentes:6d} de {total} pixels diferentes ({100 * diferentes / total:.3f} %)")


def passos(args):
    regiao = regiao_de(args.regiao, None)
    quantos = len([f for f in os.listdir(args.original) if re.fullmatch(rf"oficial_{re.escape(args.prefixo)}_\d+\.png", f)])
    print("linhas: passo do original; colunas: passo do port")
    for i in range(quantos):
        a = os.path.join(args.original, f"oficial_{args.prefixo}_{i:02d}.png")
        linha = []
        for j in range(quantos):
            b = os.path.join(args.port, f"porte_{args.prefixo}_{j:02d}.png")
            linha.append(contar(a, b, regiao))
        melhor = min(range(quantos), key=lambda j: linha[j])
        print(f"{i:02d}: " + " ".join(f"{v:6d}" for v in linha) + f"   melhor port {melhor:02d} (deslocamento {melhor - i:+d})")


def par(args):
    regiao = regiao_de(args.regiao, (0, 0, 1920, 1080))
    diferentes = contar(args.original, args.port, regiao)
    empilhar(args.original, args.port, regiao, args.saida, args.escala)
    print(f"{diferentes} de {regiao[2] * regiao[3]} pixels diferentes")


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = parser.add_subparsers(dest="modo", required=True)
    v = sub.add_parser("vidro")
    v.add_argument("original"); v.add_argument("port"); v.add_argument("saida")
    v.add_argument("--regiao"); v.add_argument("--passo-x", type=int, default=300); v.add_argument("--escala", type=int, default=1)
    v.set_defaults(f=vidro)
    s = sub.add_parser("passos")
    s.add_argument("original"); s.add_argument("port"); s.add_argument("prefixo"); s.add_argument("--regiao")
    s.set_defaults(f=passos)
    p = sub.add_parser("par")
    p.add_argument("original"); p.add_argument("port"); p.add_argument("saida"); p.add_argument("--regiao"); p.add_argument("--escala", type=int, default=2)
    p.set_defaults(f=par)
    args = parser.parse_args()
    args.f(args)


if __name__ == "__main__":
    main()
