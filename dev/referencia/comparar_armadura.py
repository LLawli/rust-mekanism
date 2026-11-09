#!/usr/bin/env python3
"""Compara, pixel a pixel, a armadura vestida no original (`oficial_equipamento.sh`: `equip_oficial_<nome>.png`) e no port (`horizon2/h2_armadura_vestida.py`: `equip_port_<nome>.png`), nas
mesmas etapas de `cenas/equip_vestida.txt`: o jogador, o armor stand e o husk diante do mesmo chão e da mesma parede de concreto preto, com a mesma câmera. Usa o ImageMagick (`compare`, `convert`).

Para cada etapa, três recortes (o husk à esquerda, o jogador no meio, o armor stand à direita), em pixels da janela de 1920 por 1080: conta os pixels que diferem (`-fuzz` de 4 %, a tolerância
do filtro de textura e da luz) e grava o lado a lado (`lado_<nome>.png`, original à esquerda e port à direita) e a diferença (`dif_<nome>.png`). A primeira pessoa e o inventário comparam a janela inteira
menos o HUD (o chat e a barra), e a tela do inventário só o boneco.

Uso: comparar_armadura.py PASTA_DO_ORIGINAL PASTA_DO_PORT PASTA_DA_SAIDA [NOME ...]"""
import os
import subprocess
import sys

# (x, y, largura, altura) de cada figura na janela de 1920 por 1080, vista de frente a 4 blocos.
FIGURAS = {"husk": (300, 380, 480, 560), "jogador": (780, 380, 360, 560), "stand": (1140, 380, 480, 560)}
TELAS = {"primeira": {"janela": (1100, 600, 820, 480)}, "inventario": {"boneco": (710, 238, 200, 284)}}


def contar(original, port, caixa, saida):
    x, y, w, h = caixa
    recorte = f"{w}x{h}+{x}+{y}"
    diff = subprocess.run(
        ["compare", "-metric", "AE", "-fuzz", "4%", f"{original}[{recorte}]", f"{port}[{recorte}]", saida],
        capture_output=True, text=True)
    # `compare` escreve a métrica no stderr e sai com 1 quando difere.
    try:
        return int(float(diff.stderr.strip().split()[0])), w * h
    except (ValueError, IndexError):
        return -1, w * h


def main():
    pasta_o, pasta_p, saida = sys.argv[1:4]
    nomes = sys.argv[4:]
    os.makedirs(saida, exist_ok=True)
    total_ruim = 0
    for arquivo in sorted(os.listdir(pasta_o)):
        if not (arquivo.startswith("equip_oficial_") and arquivo.endswith(".png")):
            continue
        nome = arquivo[len("equip_oficial_"):-4]
        if nomes and nome not in nomes:
            continue
        port = os.path.join(pasta_p, f"equip_port_{nome}.png")
        if not os.path.exists(port):
            print(f"FALTA {nome} no port")
            continue
        original = os.path.join(pasta_o, arquivo)
        caixas = FIGURAS
        for sufixo, c in TELAS.items():
            if nome.endswith(sufixo):
                caixas = c
        linhas = []
        for figura, caixa in caixas.items():
            diferentes, area = contar(original, port, caixa, os.path.join(saida, f"dif_{nome}_{figura}.png"))
            linhas.append(f"{figura} {diferentes}/{area} ({100.0 * diferentes / area:.2f}%)")
            if diferentes != 0:
                total_ruim += 1
        subprocess.run(["convert", original, port, "+append", "-resize", "50%", os.path.join(saida, f"lado_{nome}.png")])
        print(f"{nome}: " + "; ".join(linhas))
    print("recortes com diferença:", total_ruim)


if __name__ == "__main__":
    main()
