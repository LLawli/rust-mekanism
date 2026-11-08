#!/usr/bin/env python3
"""A tabela da temperatura do Heat Generator por altura, original (o rastro do Java) contra o port (a leitura em jogo de `h2_temperatura.py`,
`H_DUMP`), como imagem: um SVG que o `rsvg-convert` leva a PNG.

Uso: h2_temperatura_midia.py BIOMA LEITURA.json SAIDA.svg [TICK]   (BIOMA: desert|frozen; TICK: o tick lido, 40 por padrão)
"""
import json, os, sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import h2_identity as h

TITLES = {"desert": "Deserto", "frozen": "Oceano congelado"}


def main():
    biome, reading, out = sys.argv[1:4]
    tick = int(sys.argv[4]) if len(sys.argv) > 4 else 40
    with open(f"{h.TRACES}/heat_ambient_game_{biome}.jsonl") as f:
        lines = [json.loads(line) for line in f]
    header, body = lines[0], lines[1:]
    with open(reading) as f:
        port = {r["y"]: r for r in json.load(f)}
    rows = []
    for index, gen in enumerate(header["gens"]):
        y = gen["pos"][1]
        if y not in port:
            continue
        want = body[tick]["gens"][index]
        rows.append((y, want["ambient"], want["temperature"], port[y]["heat"] / 10.0, port[y]["heat"] == want["heat"]))
    width, height = 980, 120 + 44 * len(rows)
    svg = [
        f'<svg xmlns="http://www.w3.org/2000/svg" width="{width}" height="{height}" font-family="DejaVu Sans, sans-serif">',
        f'<rect width="{width}" height="{height}" fill="#14161a"/>',
        f'<text x="24" y="40" font-size="24" fill="#f2f2f2">Heat Generator: temperatura por altura ({TITLES.get(biome, biome)}, tick {tick})</text>',
        '<text x="24" y="68" font-size="15" fill="#9aa0a8">Mesma coluna, mesmo bioma, nível do mar -63 (mundo plano): o ar esfria acima de seaLevel + 17 = -46</text>',
    ]
    cols = [24, 150, 360, 600, 810]
    heads = ["Altura (y)", "Ar do bloco (K)", "Original (K)", "Port (K)", "Calor igual ao bit"]
    for x, text in zip(cols, heads):
        svg.append(f'<text x="{x}" y="104" font-size="15" fill="#9aa0a8">{text}</text>')
    for i, (y, ambient, original, ported, same) in enumerate(rows):
        top = 130 + 44 * i
        color = "#7bd88f" if same else "#ff6b6b"
        svg.append(f'<rect x="16" y="{top - 22}" width="{width - 32}" height="40" fill="{"#1b1e24" if i % 2 == 0 else "#14161a"}"/>')
        cells = [str(y), f"{ambient:.4f}", f"{original:.6f}", f"{ported:.6f}", "sim" if same else "NAO"]
        for j, (x, text) in enumerate(zip(cols, cells)):
            fill = color if j == 4 else "#f2f2f2"
            svg.append(f'<text x="{x}" y="{top + 4}" font-size="20" fill="{fill}">{text}</text>')
    svg.append("</svg>")
    with open(out, "w") as f:
        f.write("\n".join(svg))


if __name__ == "__main__":
    main()
