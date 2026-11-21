#!/usr/bin/env python3
"""Compara os prints da cena dos Fluid Tanks (frente `tanques`, 6.ª rodada): o original (`oficial_tanques6.sh`: `oficial_<tiro>.png`) e o port (`horizon2/h2_blocos.py cena`
com `REPETIR=N`: `porte_<tiro>.png` e `porte_<tiro>_r<k>.png`, o mesmo estado em fases diferentes da animação do sprite), do roteiro de `gerar_cena_tanques.py`.

Por tiro e por recorte: os pixels com diferença de canal acima de 8 e acima de 24 contra o print do port de fase mais próxima, e a linha mais alta em que cada tanque difere
do tanque vazio do mesmo jogo (o topo do fluido). Escreve `resumo.json` e, por recorte, o empilhado ampliado (o port em cima, o original no meio, a máscara embaixo).

Uso (no desktop, que tem PIL e numpy): comparar_tanques6.py PASTA_DO_ORIGINAL PASTA_DO_PORT PASTA_DE_SAIDA
"""
import glob
import json
import os
import sys

import numpy as np
from PIL import Image

# Os recortes, em pixels do print de 1920 por 1080: a fila dos tanques, a coluna de dois andares da válvula, a barra de atalhos e a mão.
FILA = (420, 270, 1500, 515)
COLUNA = (340, 30, 1580, 515)
BARRA = (595, 990, 1325, 1080)
MAO = (1250, 700, 1920, 1080)
RECORTES = {
    "vazios": {"fila": FILA},
    "agua_niveis": {"fila": FILA},
    "fluidos": {"fila": FILA},
    "animando": {"fila": FILA},
    "valvula": {"coluna": COLUNA},
    "ativos": {"fila": FILA},
    "quebrados": {"fila": FILA},
    "item_agua": {"barra": BARRA, "mao": MAO},
    "item_lava": {"barra": BARRA, "mao": MAO},
}
# As faixas de x de cada tanque na fila (da esquerda, x 63, para a direita, x 59).
TANQUES = ((425, 650), (655, 860), (872, 1050), (1065, 1262), (1268, 1495))


def abre(caminho):
    return np.asarray(Image.open(caminho).convert("RGB")).astype(np.int16)


def recorta(img, caixa):
    x0, y0, x1, y1 = caixa
    return img[y0:y1, x0:x1]


def diferenca(a, b):
    return np.abs(a - b).max(axis=2)


# As marcas do tier do vidro do tanque básico, com o sombreado das faces norte e sul (0,8): a textura apagada (`fluid_tank_side_off`: 188 e 210 vezes a cor do tier,
# (95, 255, 184)) e a acesa (`fluid_tank_side_on`: 225 e 227).
MARCAS = {"apagadas": ((56, 150, 109), (62, 168, 122)), "acesas": ((67, 180, 130), (68, 182, 131))}


def marcas(img, caixa):
    """Quantos pixels do recorte têm a cor das marcas apagadas e a das acesas (até 3 níveis de cada canal)."""
    parte = recorta(img, caixa)
    return {nome: int(sum((np.abs(parte - np.array(cor)).max(axis=2) <= 3).sum() for cor in cores)) for nome, cores in MARCAS.items()}


def topo(img, vazio):
    """A linha mais alta, por tanque, em que o print difere do tanque vazio do mesmo jogo por mais de 24 em pelo menos 12 pixels."""
    d = diferenca(img, vazio) > 24
    linhas = []
    for x0, x1 in TANQUES:
        cheias = np.nonzero(d[FILA[1]:FILA[3], x0:x1].sum(axis=1) >= 12)[0]
        linhas.append(int(cheias[0]) + FILA[1] if len(cheias) else None)
    return linhas


def main():
    original, port, saida = sys.argv[1:4]
    os.makedirs(saida, exist_ok=True)
    resumo = {}
    vazio_o = abre(os.path.join(original, "oficial_vazios.png"))
    vazio_p = abre(os.path.join(port, "porte_vazios.png"))
    for tiro, recortes in RECORTES.items():
        o = abre(os.path.join(original, f"oficial_{tiro}.png"))
        fases = sorted(glob.glob(os.path.join(port, f"porte_{tiro}.png")) + glob.glob(os.path.join(port, f"porte_{tiro}_r*.png")))
        linha = resumo[tiro] = {"fases": len(fases)}
        for nome, caixa in recortes.items():
            melhor = None
            for fase in fases:
                p = abre(fase)
                d = diferenca(recorta(o, caixa), recorta(p, caixa))
                conta = int((d > 8).sum())
                if melhor is None or conta < melhor[0]:
                    melhor = (conta, fase, p, d)
            conta, fase, p, d = melhor
            total = d.size
            linha[nome] = {"pixels": total, "acima_de_8": conta, "acima_de_24": int((d > 24).sum()), "maxima": int(d.max()), "fase": os.path.basename(fase)}
            mascara = np.repeat(((d > 8) * 255)[:, :, None], 3, axis=2)
            faixa = np.full((6, d.shape[1], 3), 255, dtype=np.int16)
            pilha = np.concatenate([recorta(p, caixa), faixa, recorta(o, caixa), faixa, mascara]).astype(np.uint8)
            Image.fromarray(pilha).save(os.path.join(saida, f"lado_{tiro}_{nome}.png"))
            if tiro in ("valvula", "ativos"):
                linha["marcas_original"] = marcas(o, caixa)
                linha["marcas_port"] = marcas(p, caixa)
            if nome == "fila" and tiro not in ("vazios", "ativos", "quebrados"):
                linha["topo_original"] = topo(o, vazio_o)
                linha["topo_port"] = topo(p, vazio_p)
    json.dump(resumo, open(os.path.join(saida, "resumo.json"), "w"), indent=1)
    for tiro, linha in resumo.items():
        print(tiro, json.dumps(linha))


if __name__ == "__main__":
    main()
