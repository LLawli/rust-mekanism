#!/usr/bin/env python3
"""Conta os pixels diferentes nos cantos transparentes do suporte da aba de redstone, original contra port, por caso de `cena_telas.casos_redstone`.

    medir_cantos_aba.py PASTA_ORIGINAL PASTA_PORT ESCALA...

`PASTA_ORIGINAL` tem os `oficial_<caso>_e<escala>.png` e `PASTA_PORT` os `port_<caso>_e<escala>.png` (as mesmas pastas do `comparar_lote.py`). Cada linha diz quantos
pixels dos cantos (`comparar_tela.cantos_do_suporte`, a escala toda de cada pixel de GUI) diferem em algum canal e a maior diferença: o aviso multiplica o mundo atrás da tela
nos cantos (`GUI_TEXTURED_DST_COLOR`), e sem o blend o port deixava até 10 níveis (54 pixels em guiScale 3). A diferença de 1 nível que sobra é o mundo atrás, que os estados sem
aviso (sem multiplicação nenhuma) também têm."""
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import cena_telas  # noqa: E402
import comparar_tela as ct  # noqa: E402


def medir(original, port, escala):
    ox, oy = ct.origem_da_tela(original[0], original[1], escala)
    diferentes = maior = 0
    for gx, gy in ct.cantos_do_suporte(ct.ABA_REDSTONE):
        for dy in range(escala):
            for dx in range(escala):
                i = ((oy + gy * escala + dy) * original[0] + ox + gx * escala + dx) * 4
                d = max(abs(original[2][i + k] - port[2][i + k]) for k in range(3))
                if d:
                    diferentes += 1
                    maior = max(maior, d)
    return diferentes, maior


if __name__ == "__main__":
    pasta_original, pasta_port, escalas = sys.argv[1], sys.argv[2], [int(e) for e in sys.argv[3:]]
    for escala in escalas:
        for caso in cena_telas.casos_redstone():
            nome = caso["nome"]
            try:
                original = ct.ler_png(os.path.join(pasta_original, f"oficial_{nome}_e{escala}.png"))
                port = ct.ler_png(os.path.join(pasta_port, f"port_{nome}_e{escala}.png"))
            except OSError:
                continue
            diferentes, maior = medir(original, port, escala)
            print(f"{nome}_e{escala}: {diferentes} pixels, maior diferença {maior}")
