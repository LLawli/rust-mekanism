#!/usr/bin/env python3
"""A metade que fica com a unidade ímpar do `break_end_odd`, em jogo, em várias posições (frente `pendencias-rede`, Fase 3, decisão 155).

    H_NAME=horizon PASTA=<pasta do game.log> h2_metade_impar.py tubo|cano|cabo PREVISOES.txt

`PREVISOES.txt` são as linhas `ODD [x, y, z] [(1, A), (3, B)]` que o teste da lib imprime (`ODD_ORIGINS="x,y,z;..." cargo test -p machinery-basis-core --test chemical_network_trace
-- --ignored --nocapture predict`, ou o `energy_network_trace` e o `fluid_network_trace`): a quantidade que a rede do cano 1 (o avançado) e a do cano 3 têm no fim, com a linha de
cinco postos na origem. Para cada origem o roteiro repete o `break_end_odd` do original (cinco blocos, o do meio-esquerdo avançado, 3.001 de recurso, 1.001 de energia no cabo, e
as quebras do 4, do 0 e do 2), com o servidor parado e `tick step`, e lê a sonda (`MTN`, `MBP` ou `MBE`): a rede com a capacidade maior é a do cano avançado. A unidade ímpar
vai para uma metade ou para a outra conforme a posição (a ordem de `positionedTransmitters`, `Long2ObjectOpenHashMap` por `BlockPos.asLong`), e o jogo tem de dar a metade que a lib
prevê para aquela posição.
"""
import ast, json, os, re, subprocess, sys, time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
os.environ.setdefault("H_NAME", "horizon")
import h2_identity as h

PASTA = os.environ.get("PASTA", "~/.cache/rust-mekanism/restos")
KINDS = {
    "tubo": {
        "prefix": "MTN",
        "basic": "mekanism:basic_pressurized_tube",
        "advanced": "mekanism:advanced_pressurized_tube",
        "push": 'data merge block %s {probe_push:{chem:"mekanism:hydrogen",amount:3001}}',
        "amount": 1,
        "capacity": 2,
        "size": 4,
    },
    "cano": {
        "prefix": "MBP",
        "basic": "mekanism:basic_mechanical_pipe",
        "advanced": "mekanism:advanced_mechanical_pipe",
        "push": 'data merge block %s {probe_push:{fluid:"minecraft:water",amount:1001}}',
        "amount": 1,
        "capacity": 2,
        "size": 4,
    },
    "cabo": {
        "prefix": "MBE",
        "basic": "mekanism:basic_universal_cable",
        "advanced": "mekanism:advanced_universal_cable",
        "push": "data merge block %s {energy:1001}",
        "amount": 0,
        "capacity": 1,
        "size": 3,
    },
}


def cmd(text):
    try:
        return h.command(text)[0]
    except (KeyError, IndexError):
        return ""


def wait(n):
    h.ctl("wait", str(n))


def txt(p):
    return "%d %d %d" % tuple(p)


def log_lines(prefix):
    out = subprocess.run(["ssh", "desktop", f"grep -a '^{prefix} ' {PASTA}/game.log | cut -c{len(prefix) + 2}-"], capture_output=True, text=True, timeout=120).stdout
    rows = []
    for line in out.splitlines():
        try:
            rows.append(json.loads(line))
        except ValueError:
            pass
    return rows


def step(n):
    """`tick step n` e a espera de `0.35 + 0.05 * n` s que os ticks levam (ai-memory `gotchas/teste-em-jogo-tick-step-e-reabrir-mundo`)."""
    cmd("tick step %d" % n)
    time.sleep(0.35 + 0.05 * n)


def predictions(path):
    out = {}
    for line in open(path):
        m = re.match(r"ODD (\[[^\]]*\]) (\[.*\])\s*$", line.strip())
        if m:
            origin = tuple(ast.literal_eval(m.group(1)))
            halves = dict(ast.literal_eval(m.group(2).replace("(", "(").replace(")", ")")))
            out[origin] = halves
    return out


def run_at(kind, origin):
    """O `break_end_odd` com o primeiro bloco em `origin`; devolve (quantidade da rede do avançado, quantidade da rede do básico) lidas da sonda no fim."""
    k = KINDS[kind]
    ox, oy, oz = origin
    cmd("tick unfreeze")
    cmd("fill %d %d %d %d %d %d minecraft:stone" % (ox - 2, oy - 1, oz - 2, ox + 6, oy - 1, oz + 2))
    cmd("fill %d %d %d %d %d %d minecraft:air" % (ox - 2, oy, oz - 2, ox + 6, oy + 2, oz + 2))
    cmd("tp @s %d %d %d" % (ox + 2, oy + 2, oz - 3))
    wait(10)
    cmd("tick freeze")
    time.sleep(1.0)
    for x in range(5):
        cmd("setblock %s %s" % (txt((ox + x, oy, oz)), k["advanced"] if x == 1 else k["basic"]))
    step(4)
    cmd(k["push"] % txt((ox, oy, oz)))
    step(4)
    for x in (4, 0, 2):
        cmd("setblock %s minecraft:air" % txt((ox + x, oy, oz)))
        step(6)
    step(2)
    cmd("tick unfreeze")
    rows = [r for r in log_lines(k["prefix"]) if r.get("runs")]
    last = rows[-1]["nets"]
    # As duas redes de um bloco cada (o 1 e o 3): a de capacidade maior é a do avançado.
    halves = sorted((n for n in last if n[k["size"]] == 1), key=lambda n: -n[k["capacity"]])
    if len(halves) != 2:
        return None
    return halves[0][k["amount"]], halves[1][k["amount"]]


def main():
    kind = sys.argv[1]
    expected = predictions(sys.argv[2])
    cmd("gamemode creative @s")
    good = 0
    seen = set()
    for origin, halves in expected.items():
        got = run_at(kind, origin)
        want = (halves[1], halves[3])
        seen.add(got[0] if got else None)
        ok = got == want
        good += ok
        print(("OK   " if ok else "DIFF ") + f"{kind} em {origin}: a rede do avançado tem {got[0] if got else None}, a do básico {got[1] if got else None}; a lib prevê {want}")
    print(f"{good} de {len(expected)} iguais ao previsto; metades vistas no avançado: {sorted(x for x in seen if x is not None)}")
    sys.exit(0 if good == len(expected) and len(seen) == 2 else 1)


if __name__ == "__main__":
    main()
