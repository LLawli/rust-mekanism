#!/usr/bin/env python3
"""Compara quantas partículas por segundo a máquina acesa solta no original e no port, e as põe contra o que o sorteio do `ClientLevel.animateTick` dá em média.

`ClientLevel.animateTick` sorteia, por tick, 667 posições a 16 blocos do jogador e 667 a 32: cada coordenada é `jogador + nextInt(r) - nextInt(r)`, então a chance de uma posição ser
a de um bloco a `(dx, dy, dz)` do jogador é, por eixo, `(r - |d|) / r^2` (zero a partir de `r`). Cada vez que o sorteio cai no bloco aceso, ele solta as partículas do `AttributeParticleFX`
(2 por chamada), então a esperança por tick de cada tipo é `667 * (P16 + P32)`, e o número de chamadas em N ticks é quase um Poisson (a variância é essa mesma esperança).

Uso: particulas_taxa.py ORIGINAL.jsonl PORT.jsonl
  ORIGINAL.jsonl  `rastros/particulas_taxa_oficial.jsonl` (linhas `resumo` da `CenaParticulas`: `ativos` e `totais` por classe)
  PORT.jsonl      a saída de `horizon2/h2_particulas.py` (linhas `resumo`: `ticks` e `particulas`)
O deslocamento do jogador ao bloco é o das etapas (3 blocos em z, 0 em x e y).
"""
import json, sys

DESLOCAMENTO = (0, 0, 3)
SORTEIOS = 667


def chance_por_eixo(d, r):
    return (r - abs(d)) / (r * r) if abs(d) < r else 0.0


def chance(deslocamento, r):
    p = 1.0
    for d in deslocamento:
        p *= chance_por_eixo(d, r)
    return p


def esperanca_por_tick(deslocamento=DESLOCAMENTO):
    """Chamadas do `animateTick` do bloco por tick: `667 * (P16 + P32)`."""
    return SORTEIOS * (chance(deslocamento, 16) + chance(deslocamento, 32))


def linhas(caminho):
    return {l["nome"]: l for l in map(json.loads, open(caminho, encoding="utf-8")) if l.get("e") == "resumo"}


def main():
    original, port = linhas(sys.argv[1]), linhas(sys.argv[2])
    por_tick = esperanca_por_tick()
    print(f"esperança: {por_tick:.5f} chamadas por tick ({por_tick * 20:.3f} por segundo, por tipo de partícula)")
    print(f"{'etapa':22} {'jogo':9} {'ticks':>6} {'chamadas':>9} {'esperadas':>10} {'desvios':>8} {'por segundo':>12}")
    for nome in ("chamber", "heat", "chamber_sem_efeitos"):
        for jogo, base in (("original", original), ("port", port)):
            if nome not in base:
                continue
            linha = base[nome]
            if jogo == "original":
                ticks = linha["ativos"]
                chamadas = max(linha["totais"].values(), default=0)
            else:
                ticks = linha["ticks"]
                chamadas = linha["chamadas"]
            esperadas = por_tick * ticks if linha.get("efeitos", True) and nome != "chamber_sem_efeitos" else 0.0
            desvios = (chamadas - esperadas) / esperadas ** 0.5 if esperadas else 0.0
            print(f"{nome:22} {jogo:9} {ticks:6d} {chamadas:9d} {esperadas:10.1f} {desvios:+8.2f} {chamadas / ticks * 20 if ticks else 0:12.3f}")


if __name__ == "__main__":
    main()
