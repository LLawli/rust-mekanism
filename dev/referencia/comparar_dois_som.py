#!/usr/bin/env python3
"""O silêncio no convidado, o original contra o port: do original, as linhas `dois: tile {...}` do `convidado.log` de `oficial_dois.sh` (o tile do mundo do cliente do convidado, um por tick: aceso,
fator de volume, silenciado); do port, os blocos `observar NOME:` do `port_dois.log` de `horizon2/h2_dois_roteiro.py` (as linhas `MEKSOUND tile t=... lit=... fully_muffled=... factor=...`, que o
port só imprime quando algo muda; precisa de uma cópia de teste com `TRACE = true` em `tile_sound.rs`). Cada lado vira a lista das MUDANÇAS de estado `(aceso, silenciado, fator)` com o tempo em ticks
do cliente desde a primeira mudança com a máquina acesa. A cena é a mesma (os mesmos `espera`), então os intervalos entre as mudanças têm de bater: o original pelo relógio do mundo do cliente, o port
pelo `game_time` do cliente.

Uso: comparar_dois_som.py CONVIDADO.log_DO_ORIGINAL PORT_dois.log [PORT_dois.log ...]"""
import json, re, sys


def original(path):
    cenas = {}
    for linha in open(path, encoding="utf-8", errors="replace"):
        m = re.search(r"dois: tile (\{.*\})", linha)
        if not m:
            continue
        d = json.loads(m.group(1))
        base, _, _ = d["nome"].rpartition("_t")
        if "aceso" not in d:
            continue
        cenas.setdefault(base, []).append((d["gt"], d["aceso"], d["mudo"], d["fator"]))
    saida = {}
    for nome, estados in cenas.items():
        mudancas, ultimo = [], None
        for gt, aceso, mudo, fator in estados:
            chave = (aceso, mudo, round(fator, 4))
            if chave != ultimo:
                mudancas.append((gt,) + chave)
                ultimo = chave
        saida[nome] = mudancas
    return saida


def port(paths):
    saida = {}
    for path in paths:
        nome = None
        for linha in open(path, encoding="utf-8", errors="replace"):
            m = re.match(r"observar (\S+):", linha)
            if m:
                nome = m.group(1)
                saida[nome] = []
                continue
            m = re.search(r"MEKSOUND tile t=(\d+) pos=\S+ \S+ \S+ lit=(\w+) fully_muffled=(\w+) factor=([\d.]+)", linha)
            if m and nome:
                saida[nome].append((int(m.group(1)), m.group(2) == "true", m.group(3) == "true", round(float(m.group(4)), 4)))
    return saida


def relativo(mudancas):
    # O zero é a primeira mudança com a máquina acesa.
    zero = next((m[0] for m in mudancas if m[1]), mudancas[0][0] if mudancas else 0)
    return [(m[0] - zero,) + m[1:] for m in mudancas]


def main():
    o, p = original(sys.argv[1]), port(sys.argv[2:])
    for nome in sorted(set(o) | set(p)):
        print(f"== {nome}")
        print("  original:", [(t, "aceso" if a else "apagado", "mudo" if m else "som", f) for t, a, m, f in relativo(o.get(nome, []))])
        print("  port    :", [(t, "aceso" if a else "apagado", "mudo" if m else "som", f) for t, a, m, f in relativo(p.get(nome, []))])


if __name__ == "__main__":
    main()
