#!/usr/bin/env python3
"""Compara, pixel a pixel, a tela da Security Desk (`GuiSecurityDesk`, 176 por 230) do anfitrião no original (`oficial_dois.sh`: `oficial_dois_<nome>.png`) e no port
(`horizon2/h2_dois_roteiro.py`: `port_anf_<nome>.png`), nos estados do roteiro `cenas/dois_mesa.txt`. O nome do print termina na escala (`_2` ou `_3`).

Conta os pixels diferentes de cada elemento da tela (as regiões de `REGIOES`, em pixels de GUI relativos ao canto da tela) e do resto do painel, e, fora do painel, os da
faixa em volta (o tooltip de um botão sai do painel; o mundo atrás é a caixa de concreto preto nos dois jogos). Os quatro cantos do painel são transparentes no sprite.

Uso: comparar_mesa.py PASTA_DO_ORIGINAL PASTA_DO_PORT [--saida PASTA] [--tolerancia N] [--ignorar regiao,regiao]
Com `--saida`, grava para cada print com diferença uma imagem só com os pixels diferentes (brancos) do painel e da faixa."""
import argparse
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import comparar_tela as ct

LARGURA, ALTURA = 176, 230
FAIXA = 60
# (x, y, largura, altura): os elementos de `GuiSecurityDesk`.
REGIOES = {
    "titulo": (4, 4, 168, 9),
    "lista": (13, 13, 122, 42),
    "jogadores_confiaveis": (13, 57, 122, 9),
    "adicionar": (1, 67, 33, 13),
    "caixa": (34, 67, 101, 13),
    "tirar": (13, 81, 122, 20),
    "modo_texto": (13, 103, 122, 9),
    "botoes_de_modo": (13, 113, 122, 16),
    "suporte_de_cima": (141, 13, 26, 37),
    "suporte_do_meio": (141, 54, 26, 34),
    "suporte_de_baixo": (141, 92, 26, 37),
    "inventario_e_dono": (8, 136, 162, 9),
    "slots_do_jogador": (7, 147, 162, 76),
}
CANTO = 3


def regiao_de(x, y):
    for nome, (rx, ry, rw, rh) in REGIOES.items():
        if rx <= x < rx + rw and ry <= y < ry + rh:
            return nome
    return "painel"


def comparar(original, port, escala, tolerancia):
    largura, altura, a = original
    largura_b, altura_b, b = port
    if (largura, altura) != (largura_b, altura_b):
        raise ValueError("os prints têm tamanhos diferentes")
    gui_l, gui_a = -(-largura // escala), -(-altura // escala)
    esquerda, topo = (gui_l - LARGURA) // 2, (gui_a - ALTURA) // 2
    contagem, diferentes = {}, []
    for py in range(max(0, (topo - FAIXA) * escala), min(altura, (topo + ALTURA + FAIXA) * escala)):
        gy = py // escala - topo
        for px in range(max(0, (esquerda - FAIXA) * escala), min(largura, (esquerda + LARGURA + FAIXA) * escala)):
            i = (py * largura + px) * 4
            if all(abs(a[i + c] - b[i + c]) <= tolerancia for c in range(3)):
                continue
            gx = px // escala - esquerda
            if 0 <= gx < LARGURA and 0 <= gy < ALTURA:
                canto = (gx < CANTO or gx >= LARGURA - CANTO) and (gy < CANTO or gy >= ALTURA - CANTO)
                nome = "cantos" if canto else regiao_de(gx, gy)
            else:
                nome = "fora"
            contagem[nome] = contagem.get(nome, 0) + 1
            diferentes.append((px, py))
    return contagem, diferentes, (esquerda, topo)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("original")
    parser.add_argument("port")
    parser.add_argument("--saida")
    parser.add_argument("--tolerancia", type=int, default=0)
    parser.add_argument("--ignorar", default="cantos")
    args = parser.parse_args()
    ignorar = set(args.ignorar.split(","))
    ruim = 0
    for arquivo in sorted(os.listdir(args.original)):
        if not (arquivo.startswith("oficial_dois_mesa_") and arquivo.endswith(".png")):
            continue
        nome = arquivo[len("oficial_dois_"):-4]
        caminho_port = os.path.join(args.port, f"port_anf_{nome}.png")
        if not os.path.exists(caminho_port):
            print(f"FALTA {nome} no port")
            ruim += 1
            continue
        escala = int(nome.rsplit("_", 1)[1])
        original, port = ct.ler_png(os.path.join(args.original, arquivo)), ct.ler_png(caminho_port)
        contagem, diferentes, _ = comparar(original, port, escala, args.tolerancia)
        reais = {n: v for n, v in contagem.items() if n not in ignorar}
        ruim += bool(reais)
        print(f"{'DIFF' if reais else 'OK  '} {nome}: {reais or 0}" + (f" (ignorado: { {n: v for n, v in contagem.items() if n in ignorar} })" if len(reais) != len(contagem) else ""))
        if args.saida and reais:
            os.makedirs(args.saida, exist_ok=True)
            largura, altura, _ = original
            xs, ys = [p[0] for p in diferentes], [p[1] for p in diferentes]
            x0, y0, x1, y1 = min(xs), min(ys), max(xs) + 1, max(ys) + 1
            rgba = bytearray(b"\x00\x00\x00\xff" * ((x1 - x0) * (y1 - y0)))
            for px, py in diferentes:
                i = ((py - y0) * (x1 - x0) + (px - x0)) * 4
                rgba[i:i + 3] = b"\xff\xff\xff"
            ct.gravar_png(os.path.join(args.saida, f"dif_{nome}.png"), x1 - x0, y1 - y0, bytes(rgba))
            print(f"     caixa das diferenças: x {x0}..{x1}, y {y0}..{y1} (pixels do print)")
    print(f"{ruim} prints com diferença")
    sys.exit(1 if ruim else 0)


if __name__ == "__main__":
    main()
