#!/usr/bin/env python3
"""Compara, por pixel, as telas dos tanques (`GuiFluidTank` e `GuiChemicalTank`) do original e do port, e empilha os recortes (o port em cima, o original embaixo).

    comparar_tanques.py PASTA_ORIGINAL PASTA_PORT SAIDA ESCALA... [--so caso,caso]

Roda no desktop, que tem PIL e numpy (os prints são de 1920 por 1080 e os casos animados têm dezenas deles). Os prints vêm de `cena_telas.py`: `oficial_<caso>_e<escala>[_k].png`
do `CenaTelas` e `port_<caso>_e<escala>[_k].png` do Horizon, os casos de `cena_telas.casos_tanques()`.

O recorte é a tela de 176 por 166 com as abas dos dois lados e, nos casos com o cursor, o tooltip. Cada pixel do recorte cai numa classe:
`tela` (o painel), `abas` (as duas colunas de 26 ao lado dele: o suporte da armadura, as abas de lados, de segurança, de redstone e do modo) e `fora` (o resto: o
tooltip e o mundo atrás). O fluido e o químico são sprites animados nos dois jogos: cada print do original é casado com o print do port que menos difere dele no
miolo do medidor ou da barra, e o resultado do caso é o do pior par.

Para cada caso e escala sai uma linha com os pixels diferentes por classe (qualquer canal diferente, e os que passam de 8 por canal), `SAIDA/<caso>_e<escala>.png`
com os dois recortes empilhados e a máscara do que difere, e `SAIDA/resumo.json`. O código de saída é 0 sem diferença na `tela` nem nas `abas`, 1 com alguma.
"""
import glob
import json
import os
import sys

import numpy as np
from PIL import Image

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import cena_telas  # noqa: E402

LARGURA, ALTURA = 176, 166
# O recorte em pixels de GUI relativos ao canto da tela: as abas de 26 dos dois lados e uma folga; com o cursor, a área onde o tooltip cai.
RECORTE = (-30, -4, 206, 170)
RECORTE_COM_CURSOR = (-30, -24, 330, 170)
# O miolo animado de cada tela: o do medidor largo (49, 19, 64 por 48) e o da barra horizontal (43, 17, 116 por 10).
ANIMADO = {"tanque": (49, 19, 113, 67), "tanque_quimico": (43, 17, 159, 27)}
LIMIAR = 8


def origem(largura, altura, escala):
    """O canto da tela no print: `AbstractContainerScreen.init` centra no tamanho de GUI (a janela dividida pela escala, para cima) e trunca."""
    return ((-(-largura // escala) - LARGURA) // 2) * escala, ((-(-altura // escala) - ALTURA) // 2) * escala


def recorta(imagem, caixa, escala):
    ox, oy = origem(imagem.shape[1], imagem.shape[0], escala)
    x0, y0, x1, y1 = (v * escala for v in caixa)
    return imagem[max(oy + y0, 0) : oy + y1, max(ox + x0, 0) : ox + x1, :3].astype(np.int16)


def classes(caixa, escala):
    """A classe de cada pixel do recorte: 0 `tela`, 1 `abas`, 2 `fora`."""
    x0, y0, x1, y1 = caixa
    ys, xs = np.mgrid[y0 * escala : y1 * escala, x0 * escala : x1 * escala]
    gx, gy = xs // escala, ys // escala
    dentro_y = (gy >= 0) & (gy < ALTURA)
    tela = dentro_y & (gx >= 0) & (gx < LARGURA)
    abas = dentro_y & (((gx >= -26) & (gx < 0)) | ((gx >= LARGURA) & (gx < LARGURA + 26)))
    return np.where(tela, 0, np.where(abas, 1, 2))


def le(caminho):
    return np.asarray(Image.open(caminho).convert("RGB"))


def quadros(pasta, prefixo, nome, escala):
    base = os.path.join(pasta, f"{prefixo}_{nome}_e{escala}")
    return sorted(glob.glob(base + ".png") + glob.glob(base + "_*.png"))


def compara_caso(c, escala, pasta_original, pasta_port, saida):
    tela = cena_telas.MAQUINAS[c["maquina"]]["tela"]
    caixa = RECORTE_COM_CURSOR if "hover" in c["args"] else RECORTE
    originais = quadros(pasta_original, "oficial", c["nome"], escala)
    ports = quadros(pasta_port, "port", c["nome"], escala)
    if not originais or not ports:
        return {"caso": c["nome"], "escala": escala, "erro": f"faltam prints: {len(originais)} do original e {len(ports)} do port"}
    recortes_port = [recorta(le(p), caixa, escala) for p in ports]
    classe = classes(caixa, escala)
    ax0, ay0, ax1, ay1 = ((v - o) * escala for v, o in zip(ANIMADO[tela], (caixa[0], caixa[1], caixa[0], caixa[1])))
    pior = None
    for caminho in originais:
        original = recorta(le(caminho), caixa, escala)
        if original.shape != recortes_port[0].shape:
            return {"caso": c["nome"], "escala": escala, "erro": f"tamanhos diferentes: {original.shape} e {recortes_port[0].shape}"}
        # O par: o print do port com menos diferença no miolo animado.
        custo = [int((np.abs(original[ay0:ay1, ax0:ax1] - port[ay0:ay1, ax0:ax1]).max(axis=2) > 0).sum()) for port in recortes_port]
        k = int(np.argmin(custo))
        diferenca = np.abs(original - recortes_port[k]).max(axis=2)
        conta = {}
        for indice, nome in enumerate(("tela", "abas", "fora")):
            trecho = diferenca[classe == indice]
            conta[nome] = {"diferentes": int((trecho > 0).sum()), "acima_de_8": int((trecho > LIMIAR).sum()), "maior": int(trecho.max()) if trecho.size else 0}
        peso = (conta["tela"]["diferentes"], conta["abas"]["diferentes"], conta["fora"]["diferentes"])
        if pior is None or peso > pior[0]:
            pior = (peso, conta, original, recortes_port[k], diferenca, os.path.basename(caminho), os.path.basename(ports[k]))
    _, conta, original, port, diferenca, nome_original, nome_port = pior
    faixa = np.full((6, original.shape[1], 3), 255, dtype=np.uint8)
    mascara = np.zeros_like(original, dtype=np.uint8)
    mascara[diferenca > 0] = (255, 255, 0)
    mascara[diferenca > LIMIAR] = (255, 0, 0)
    empilhado = np.vstack([port.astype(np.uint8), faixa, original.astype(np.uint8), faixa, mascara])
    Image.fromarray(empilhado).save(os.path.join(saida, f"{c['nome']}_e{escala}.png"))
    return {"caso": c["nome"], "escala": escala, "original": nome_original, "port": nome_port, "prints": [len(originais), len(ports)], **conta}


def main():
    args = sys.argv[1:]
    so = None
    if "--so" in args:
        i = args.index("--so")
        so = args[i + 1].split(",")
        del args[i : i + 2]
    pasta_original, pasta_port, saida = args[:3]
    os.makedirs(saida, exist_ok=True)
    resumo = []
    falhou = False
    for escala in (int(e) for e in args[3:]):
        for c in cena_telas.casos_tanques():
            if so and c["nome"] not in so:
                continue
            r = compara_caso(c, escala, pasta_original, pasta_port, saida)
            resumo.append(r)
            if "erro" in r:
                falhou = True
                print(f"ERRO {r['caso']} e{escala}: {r['erro']}", flush=True)
                continue
            ruim = r["tela"]["diferentes"] or r["abas"]["diferentes"]
            falhou = falhou or bool(ruim)
            print(
                f"{'DIF' if ruim else 'ok '} {r['caso']} e{escala}: tela {r['tela']['diferentes']} ({r['tela']['acima_de_8']} acima de 8, até {r['tela']['maior']}), "
                f"abas {r['abas']['diferentes']} ({r['abas']['acima_de_8']}, até {r['abas']['maior']}), fora {r['fora']['diferentes']} ({r['fora']['acima_de_8']}, até {r['fora']['maior']}) "
                f"[{r['original']} com {r['port']}]",
                flush=True,
            )
    with open(os.path.join(saida, "resumo.json"), "w") as arquivo:
        json.dump(resumo, arquivo, indent=1, ensure_ascii=False)
    return 1 if falhou else 0


if __name__ == "__main__":
    sys.exit(main())
