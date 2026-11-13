#!/usr/bin/env python3
"""Do log do cliente do original (`CenaLeitor`, `cenas/ar_bioma.txt`) para o rastro `rastros/ar_bioma.jsonl`: uma linha por caso, com o ar que o original leu em cada posição (o bioma, os bits da
temperatura base e da temperatura com o ajuste de altura, e o `HeatAPI.getAmbientTemp(level, pos)`) e o chat do Network Reader no condutor do meio.

Uso: ar_bioma_rastro.py LOG_DO_ORIGINAL SAIDA.jsonl   (o LOG tem só as linhas `AR` e `CHAT` do `latest.log`)
"""
import json, re, sys


def main(entrada, saida):
    casos = {}
    ordem = []
    with open(entrada) as file:
        for line in file:
            m = re.search(r" AR (\{.*\})\s*$", line)
            if m:
                row = json.loads(m.group(1))
                ordem.append(row["nome"])
                casos[row["nome"]] = {"nome": row["nome"], "y": row["y"], "sea": row["sea"], "pos": row["pos"], "chat": []}
                continue
            m = re.search(r" CHAT (\w+): ?(.*)$", line)
            if m and m.group(1) in casos:
                casos[m.group(1)]["chat"].append(m.group(2).rstrip())
    with open(saida, "w") as file:
        for nome in ordem:
            file.write(json.dumps(casos[nome], ensure_ascii=False, separators=(",", ":")) + "\n")
    print(f"{len(ordem)} casos, {sum(len(c['pos']) for c in casos.values())} posições")


if __name__ == "__main__":
    main(sys.argv[1], sys.argv[2])
