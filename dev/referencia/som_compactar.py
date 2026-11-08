#!/usr/bin/env python3
"""Encurta o `som-medido.jsonl` da `CenaSomMedida` para caber no repositório (o jj recusa arquivo novo acima de 1 MiB): tira o que a linha repete do cenário
(`cenario`) e o que ninguém lê (`original`), corta os números para 4 casas sem zeros à direita, escreve sem espaços e junta as linhas `tick` seguidas que só
diferem em `n` e `gt` (cada um um a mais) numa só, com `x` = quantas linhas ela repete a mais. Uso: som_compactar.py ENTRADA SAIDA"""
import json, sys


def short(value):
    if isinstance(value, float):
        value = round(value, 4)
        return int(value) if value == int(value) and abs(value) < 1e9 else value
    if isinstance(value, list):
        return [short(v) for v in value]
    if isinstance(value, dict):
        return {k: short(v) for k, v in value.items()}
    return value


def same(a, b):
    return {k: v for k, v in a.items() if k not in ("n", "gt", "x")} == {k: v for k, v in b.items() if k not in ("n", "gt", "x")}


out = []
with open(sys.argv[1], encoding="utf-8") as src:
    for line in src:
        if not line.strip():
            continue
        row = json.loads(line)
        if row["e"] in ("tick", "evento", "play"):
            row.pop("cenario", None)
        if row["e"] == "tick" and row.get("som"):
            row["som"].pop("original", None)
        row = short(row)
        last = out[-1] if out else None
        if row["e"] == "tick" and last and last["e"] == "tick" and same(row, last) and row["n"] == last["n"] + last.get("x", 0) + 1 and row["gt"] == last["gt"] + last.get("x", 0) + 1:
            last["x"] = last.get("x", 0) + 1
        else:
            out.append(row)
with open(sys.argv[2], "w", encoding="utf-8") as dst:
    for row in out:
        dst.write(json.dumps(row, separators=(",", ":"), ensure_ascii=False) + "\n")
