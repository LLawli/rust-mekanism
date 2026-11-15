#!/usr/bin/env python3
"""A coluna (x) do centro dos pixels da lâmina da espada de diamante num print de 1920 por 1080 da cena `transportador_partial.json` (a câmera de perto, `fila_perto_NN`): os pixels
verde-azulados (vermelho baixo, verde e azul altos e o verde perto do azul, o que o céu, de azul maior que o verde, não tem) numa faixa da imagem. Sem numpy: o `magick` recorta e entrega P6.

    coluna_da_espada.py PRINT.png [PRINT.png ...]      escreve `arquivo x_do_centro quantos_pixels`
"""
import subprocess, sys

REGIAO = (900, 480, 700, 160)


def coluna(caminho):
    x0, y0, w, h = REGIAO
    dados = subprocess.run(["magick", caminho, "-crop", f"{w}x{h}+{x0}+{y0}", "+repage", "-depth", "8", "ppm:-"], capture_output=True, check=True).stdout
    corpo = dados.split(b"\n", 3)[3]
    soma, n = 0, 0
    for i in range(0, len(corpo), 3):
        r, g, b = corpo[i], corpo[i + 1], corpo[i + 2]
        if r < 110 and g > 150 and b > 150 and g >= b - 25:
            soma += (i // 3) % w
            n += 1
    return (x0 + soma / n if n else None), n


if __name__ == "__main__":
    for caminho in sys.argv[1:]:
        x, n = coluna(caminho)
        print(caminho.split("/")[-1], "-" if x is None else f"{x:.1f}", n)
