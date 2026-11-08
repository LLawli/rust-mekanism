#!/usr/bin/env python3
"""Desenha, para cada cenário pedido, o volume do som de tile ao longo dos ticks no original e no port, um em cima do outro, e o grava como PNG (SVG por
`rsvg-convert`). O eixo x é o tick relativo ao `lit` da máquina; o y é o volume da instância (0 a 10). A linha do original sai do rastro `som_medido.jsonl`
(`CenaSomMedida`), a do port dos `MEKSOUND` do stderr (`h2_som_medida.py`): início, mudanças de volume e fim.

Uso: som_grafico.py ORIGINAL.jsonl PORT.log SAIDA.png nome [nome ...]
"""
import subprocess, sys
import som_comparar as c

W, H, ROW = 1100, 120, 150
LEFT, TOP = 150, 28
TICKS = 300


def original_series(sc):
    """(tick relativo, volume ou None) por tick."""
    ticks = sc["ticks"]
    settled = next((i for i, row in enumerate(ticks) if row["tile"] and not row["tile"]["lit"]), 0)
    ticks = ticks[settled:]
    lit_on = next((row["n"] for row in ticks if row["tile"] and row["tile"]["lit"]), 0)
    out = {}
    for row in ticks:
        sound = row["som"]
        playing = sound is not None and sound["ativo"] and not sound["parado"]
        out[row["n"] - lit_on] = sound["volume"] if playing else None
    return out


def port_series(lines):
    t = lambda text: int(c.re.search(r" t=(\d+)", text).group(1))
    lit_on = next((t(x) for x in lines if x.startswith("MEKSOUND tile") and "lit=true" in x), 0)
    events = []
    for text in lines:
        if text.startswith("MEKSOUND start"):
            events.append((t(text) - lit_on, float(c.re.search(r"volume=([\d.]+) pitch", text).group(1))))
        elif text.startswith("MEKSOUND volume"):
            events.append((t(text) - lit_on, float(c.re.search(r"-> ([\d.]+)", text).group(1))))
        elif text.startswith("MEKSOUND stop") or text.startswith("MEKSOUND ended"):
            events.append((t(text) - lit_on, None))
    out, value, i = {}, None, 0
    events.sort(key=lambda e: e[0])
    for tick in range(-5, TICKS):
        while i < len(events) and events[i][0] <= tick:
            value = events[i][1]
            i += 1
        out[tick] = value
    return out


def polyline(series, y0, vmax, color):
    points = []
    for tick in range(0, TICKS):
        v = series.get(tick)
        x = LEFT + (W - LEFT - 20) * tick / TICKS
        y = y0 + (H - 20) * (1 - (v / vmax if v is not None else 0)) if v is not None else None
        points.append((x, y))
    parts, current = [], []
    for x, y in points:
        if y is None:
            if current:
                parts.append(current)
            current = []
        else:
            current.append((x, y))
    if current:
        parts.append(current)
    return "".join(f'<polyline fill="none" stroke="{color}" stroke-width="2.5" points="{" ".join(f"{x:.1f},{y:.1f}" for x, y in part)}"/>' for part in parts)


def main():
    java = c.original(sys.argv[1])
    rust = c.port(sys.argv[2])
    out = sys.argv[3]
    names = sys.argv[4:]
    height = TOP + ROW * len(names) + 40
    svg = [f'<svg xmlns="http://www.w3.org/2000/svg" width="{W}" height="{height}" font-family="DejaVu Sans, sans-serif" font-size="13"><rect width="100%" height="100%" fill="#14161a"/>']
    svg.append(f'<text x="{LEFT}" y="20" fill="#e6e6e6">volume do som de tile por tick, relativo ao tick em que a máquina acende  (linha azul: original, linha laranja: port)</text>')
    for k, name in enumerate(names):
        y0 = TOP + ROW * k + 8
        o = original_series(java[name])
        p = port_series(rust[name]) if name in rust else {}
        vmax = max([v for v in list(o.values()) + list(p.values()) if v is not None] + [1.0])
        svg.append(f'<rect x="{LEFT}" y="{y0}" width="{W - LEFT - 20}" height="{H - 20}" fill="#1c1f25" stroke="#3a3f48"/>')
        svg.append(f'<text x="10" y="{y0 + 18}" fill="#e6e6e6" font-weight="bold">{name}</text>')
        svg.append(f'<text x="10" y="{y0 + 38}" fill="#9aa0aa">escala 0 a {vmax:g}</text>')
        svg.append(polyline(o, y0, vmax, "#4aa3ff"))
        svg.append(polyline(p, y0 + 3, vmax, "#ff9f43") if p else "")
        for tick in range(0, TICKS + 1, 50):
            x = LEFT + (W - LEFT - 20) * tick / TICKS
            svg.append(f'<text x="{x - 8:.0f}" y="{y0 + H - 4}" fill="#9aa0aa" font-size="11">{tick}</text>')
    svg.append("</svg>")
    svg_path = out.rsplit(".", 1)[0] + ".svg"
    open(svg_path, "w", encoding="utf-8").write("\n".join(svg))
    subprocess.run(["rsvg-convert", "-o", out, svg_path], check=True)


if __name__ == "__main__":
    main()
