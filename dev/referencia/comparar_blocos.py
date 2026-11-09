#!/usr/bin/env python3
"""Compara os prints da cena dos blocos de máquina: o original (`oficial_blocos.sh`: `oficial_<tiro>.png`) e o port (`horizon2/h2_blocos.py cena`: `porte_<tiro>.png`), do mesmo
roteiro `cenas/blocos.json`, de 1920 por 1080. Por tiro: os pixels que diferem na faixa onde ficam os blocos (as nuvens e o relógio do céu não valem), a caixa dessa diferença, e
uma imagem com o original em cima, o port embaixo e a diferença no fim (`lado_<tiro>.png`).

Uso: comparar_blocos.py PASTA_DO_ORIGINAL PASTA_DO_PORT PASTA_DE_SAIDA [limite_de_canal]
"""
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import comparar_tela as ct

LARGURA, ALTURA = 1920, 1080
FAIXA = (330, 760)  # as linhas onde ficam os blocos, abaixo das nuvens e acima do chão perto da câmera
LIMITE = int(sys.argv[4]) if len(sys.argv) > 4 else 24


def comparar(original, port):
    (lo, _, a), (lp, _, b) = original, port
    assert lo == lp == LARGURA
    diff = bytearray(b"\x00\x00\x00\xff" * (LARGURA * (FAIXA[1] - FAIXA[0])))
    contados = total = 0
    xmin = ymin = 10**9
    xmax = ymax = -1
    for y in range(*FAIXA):
        for x in range(LARGURA):
            i = (y * LARGURA + x) * 4
            d = max(abs(a[i] - b[i]), abs(a[i + 1] - b[i + 1]), abs(a[i + 2] - b[i + 2]))
            total += 1
            if d > LIMITE:
                contados += 1
                xmin, xmax, ymin, ymax = min(xmin, x), max(xmax, x), min(ymin, y), max(ymax, y)
                j = ((y - FAIXA[0]) * LARGURA + x) * 4
                diff[j : j + 3] = b"\xff\x40\x40"
    return contados, total, (xmin, ymin, xmax, ymax), diff


def regioes(diff, folga=12, minimo=40):
    """Agrupa os pixels que diferem em regiões (vizinhos a até `folga` pixels): para cada uma, a caixa e quantos pixels. Devolve as maiores, para dizer a causa de cada resíduo olhando o mapa."""
    altura = len(diff) // (LARGURA * 4)
    pontos = {(x // folga, y // folga) for y in range(altura) for x in range(LARGURA) if diff[(y * LARGURA + x) * 4] == 0xFF}
    grupos = []
    vistos = set()
    for p in sorted(pontos):
        if p in vistos:
            continue
        pilha, grupo = [p], []
        vistos.add(p)
        while pilha:
            gx, gy = pilha.pop()
            grupo.append((gx, gy))
            for dx in (-1, 0, 1):
                for dy in (-1, 0, 1):
                    q = (gx + dx, gy + dy)
                    if q in pontos and q not in vistos:
                        vistos.add(q)
                        pilha.append(q)
        grupos.append(grupo)
    saida = []
    for grupo in grupos:
        xs = [g[0] * folga for g in grupo]
        ys = [g[1] * folga + FAIXA[0] for g in grupo]
        saida.append(((min(xs), min(ys), max(xs) + folga, max(ys) + folga), sum(1 for _ in grupo)))
    saida.sort(key=lambda r: -r[1])
    return [r for r in saida if r[1] >= minimo // 10][:6]


def empilhar(original, port, diff, saida):
    altura = FAIXA[1] - FAIXA[0]
    folga = 6
    img = bytearray(b"\x20\x20\x20\xff" * (LARGURA * (altura * 3 + folga * 2)))
    for k, rgba in enumerate((original[2], port[2], None)):
        for linha in range(altura):
            destino = ((k * (altura + folga) + linha) * LARGURA) * 4
            if rgba is None:
                img[destino : destino + LARGURA * 4] = diff[linha * LARGURA * 4 : (linha + 1) * LARGURA * 4]
            else:
                origem = ((FAIXA[0] + linha) * LARGURA) * 4
                img[destino : destino + LARGURA * 4] = rgba[origem : origem + LARGURA * 4]
    ct.gravar_png(saida, LARGURA, altura * 3 + folga * 2, img)


def main():
    pasta_o, pasta_p, saida = sys.argv[1:4]
    os.makedirs(saida, exist_ok=True)
    nomes = sorted(f[len("oficial_") : -4] for f in os.listdir(pasta_o) if f.startswith("oficial_") and f.endswith(".png"))
    for nome in nomes:
        o = ct.ler_png(os.path.join(pasta_o, f"oficial_{nome}.png"))
        p = ct.ler_png(os.path.join(pasta_p, f"porte_{nome}.png"))
        contados, total, caixa, diff = comparar(o, p)
        empilhar(o, p, diff, os.path.join(saida, f"lado_{nome}.png"))
        print(f"{nome}: {contados} de {total} pixels ({100 * contados / total:.3f} %), caixa {caixa if contados else None}")
        if contados > total // 1000:
            for caixa_regiao, celulas in regioes(diff):
                print(f"    região {caixa_regiao}: {celulas} células de 12 por 12")


if __name__ == "__main__":
    main()
