#!/usr/bin/env python3
"""Resume, por cenário, o som das máquinas do original (`som-medido.jsonl` da `CenaSomMedida`) e do port (os blocos `=== nome` de `h2_som_medida.py`, com as
linhas `MEKSOUND`), e os põe lado a lado. Os instantes são relativos ao tick em que a máquina acende (`lit`) para o som poder ser comparado entre os dois jogos,
que não partem do mesmo tick de relógio.

Uso: som_comparar.py ORIGINAL.jsonl PORT.log [nome ...]
Cada linha de resumo: `lit` (tick em que acendeu e apagou), `inicio` (tick relativo em que o som começou e o volume), `volumes` (as mudanças), `fim` (quando acabou e como).
"""
import json, re, sys
from collections import OrderedDict


def original(path):
    """O rastro da `CenaSomMedida`, compacto (`som_compactar.py`: sem `cenario` nas linhas e com `x` para as repetidas) ou não."""
    out = OrderedDict()
    name = None
    for line in open(path, encoding="utf-8"):
        if not line.strip():
            continue
        row = json.loads(line)
        kind = row["e"]
        if kind == "cenario":
            name = row["nome"]
            out[name] = {"ticks": [], "plays": []}
            continue
        if name is None:
            continue
        row.setdefault("cenario", name)
        if kind == "tick":
            for step in range(row.get("x", 0) + 1):
                copy = dict(row)
                copy["n"] += step
                copy["gt"] += step
                out[name]["ticks"].append(copy)
        elif kind == "play":
            out[name]["plays"].append(row)
    return out


def summarize_original(sc):
    ticks = sc["ticks"]
    # Os primeiros ticks de um cenário podem mostrar o bloco do cenário anterior ou o do mundo salvo: o resumo começa na primeira vez que o tile está apagado.
    settled = next((i for i, row in enumerate(ticks) if row["tile"] and not row["tile"]["lit"]), 0)
    ticks = ticks[settled:]
    lit_on = lit_off = None
    last_lit = False
    for row in ticks:
        lit = bool(row["tile"] and row["tile"]["lit"])
        if lit and not last_lit and lit_on is None:
            lit_on = row["n"]
        if not lit and last_lit and lit_off is None:
            lit_off = row["n"]
        last_lit = lit
    base = lit_on if lit_on is not None else 0
    start = None
    volumes = []
    end = None
    last_volume = None
    for row in ticks:
        sound = row["som"]
        playing = sound is not None and sound["ativo"] and not sound["parado"]
        if playing:
            if start is None:
                start = (row["n"] - base, sound["volume"], sound["intervalo"])
                last_volume = sound["volume"]
            elif sound["volume"] != last_volume:
                volumes.append((row["n"] - base, last_volume, sound["volume"]))
                last_volume = sound["volume"]
        elif start is not None and end is None:
            end = row["n"] - base
    return {"lit": (None if lit_on is None else 0, None if lit_off is None else lit_off - base), "inicio": start, "volumes": volumes, "fim": end}


def port(path):
    out = OrderedDict()
    name = None
    for line in open(path, encoding="utf-8", errors="replace"):
        line = line.rstrip("\n")
        if line.startswith("=== "):
            name = line[4:]
            out[name] = []
        elif name and "MEKSOUND" in line:
            out[name].append(line[line.index("MEKSOUND"):])
    return out


def summarize_port(lines):
    t = lambda text: int(re.search(r" t=(\d+)", text).group(1))
    lit_on = lit_off = None
    start = None
    volumes = []
    end = None
    for text in lines:
        if text.startswith("MEKSOUND tile"):
            lit = "lit=true" in text
            if lit and lit_on is None:
                lit_on = t(text)
            if not lit and lit_on is not None and lit_off is None:
                lit_off = t(text)
    base = lit_on if lit_on is not None else 0
    for text in lines:
        if text.startswith("MEKSOUND start") and start is None:
            volume = float(re.search(r"volume=([\d.]+) pitch", text).group(1))
            start = (t(text) - base, volume)
        elif text.startswith("MEKSOUND volume"):
            a, b = re.search(r"([\d.]+) -> ([\d.]+)", text).groups()
            volumes.append((t(text) - base, float(a), float(b)))
        elif (text.startswith("MEKSOUND stop") or text.startswith("MEKSOUND ended")) and end is None:
            end = (t(text) - base, text.split()[1])
    return {"lit": (None if lit_on is None else 0, None if lit_off is None else lit_off - base), "inicio": start, "volumes": volumes, "fim": end}


def show(label, summary):
    print(f"  {label:9} lit {summary['lit']}  inicio {summary['inicio']}  volumes {summary['volumes']}  fim {summary['fim']}")


def main():
    java = original(sys.argv[1])
    rust = port(sys.argv[2]) if len(sys.argv) > 2 else {}
    only = sys.argv[3:]
    for name, sc in java.items():
        if only and name not in only:
            continue
        print(name)
        show("original", summarize_original(sc))
        if name in rust:
            show("port", summarize_port(rust[name]))
        plays = {(p["nome"], p["id"], p["categoria"], p["volume"], p["pitch"], p["atenuacao"], p["laco"], p["atraso"], p["relativo"], p["comeca_mudo"], p["distancia_atenuacao"]) for p in sc["plays"]}
        for play in sorted(plays, key=str):
            print("  play original:", play)


if __name__ == "__main__":
    main()
