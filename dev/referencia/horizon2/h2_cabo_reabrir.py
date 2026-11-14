#!/usr/bin/env python3
"""A conexão cabo com cabo depois de sair do mundo e reabri-lo (frente `cabo-forma`, a segunda rodada): a mesma cena de `testes/CenaReabrir.java` no port.

    H_NAME=horizon2 PASTA=<pasta do game.log> h2_cabo_reabrir.py montar   # monta a cena de `h2_cabo_forma.py` (o cabo com os cubos e o cabo ao sul, a reta e a esquina) e põe `PUSH` e `PULL` pelo Configurator
    H_NAME=horizon2 PASTA=<pasta do game.log> h2_cabo_reabrir.py ler ROTULO # a vista publicada (a sonda `MBV`, só no build com `trace`) de cada cabo da cena: o tipo cru, o visível e a conexão de cada lado

Depois de `montar` o jogo é fechado (`ctl.sh quit`, que salva) e reaberto no mesmo mundo; o `ler` do port sai do `game.log` do jogo reaberto. Os lados na ordem do jogo: baixo, cima, norte, sul, oeste, leste.
"""
import json, os, sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import h2_cabo_forma as f

CABOS = [(0, 0, 0), (0, 0, 1), (0, 0, 24), (1, 0, 24), (2, 0, 24), (0, 0, 30), (1, 0, 30), (0, 0, 31)]
LETRA = {"normal": "N", "push": "P", "pull": "P", "none": "N"}  # a primeira letra do `ConnectionType` que `CenaReabrir` escreve: `NORMAL` e `NONE` dão `N`, `PUSH` e `PULL` dão `P`


def linha(rotulo, c, v):
    """`cena: reabrir ROTULO x y z cli MASCARA TIPOS`, no formato do original (a máscara `getAllCurrentConnections` em hexadecimal, o tipo visível de cada lado)."""
    mascara = sum(1 << i for i, ligado in enumerate(v["conn"]) if ligado)
    tipos = "".join(LETRA[k] for k in v["kind"])
    return f"cena: reabrir {rotulo} {c[0]} {c[1]} {c[2]} cli {mascara:02x} {tipos}"


def ler(rotulo):
    h, aj = f.port()
    vistas = aj.last_views("MBV")
    for c in CABOS:
        v = vistas.get(f.mundo(c))
        print(linha(rotulo, c, v) if v else f"cena: reabrir {rotulo} {c[0]} {c[1]} {c[2]} sem vista")


if __name__ == "__main__":
    modo = sys.argv[1]
    if modo == "montar":
        f.montar()
    elif modo == "ler":
        ler(sys.argv[2])
    else:
        print(__doc__)
        sys.exit(2)
