#!/usr/bin/env python3
"""Os cabos universais no jogo contra o original (frente `cabo-jogo`, Fase 3, decisão 121).

    H_NAME=horizon PASTA=<pasta do game.log> h2_cabo_jogo.py energia CENARIO|todos   # repete os rastros energy_*.jsonl tick a tick
    H_NAME=horizon PASTA=<pasta do game.log> h2_cabo_jogo.py configurador            # o Configurator cicla um lado, o braço some e volta, o redstone reativo
    H_NAME=horizon PASTA=<pasta do game.log> h2_cabo_jogo.py puxar                   # o cabo em PULL puxa de um cubo de energia

A sonda (`TRANSMITTER_PROBE = true` na cópia de teste, `enviar_fontes.sh`) escreve no stderr do jogo (`PASTA/game.log`): `MBE {t, runs, nets: [[energia, capacidade, saiu, cabos,
aceitadores], ...]}` a cada `ServerTickEvent.Post` e `MBV {p, conn, kind, raw, net}` a cada vez que um cabo publica as conexões dele. O servidor fica parado (`tick freeze`) e anda
com `tick step 1`: cada passo dá uma linha `MBE` com `runs` verdadeiro, que se compara com a linha `estate` do rastro do mesmo tick (a energia de cada rede, a capacidade e o que ela mandou).
O rastro põe máquinas com buffer de 1.000 a 2.000.000.000 (`EnergyContainer` feito para o teste), e a Enrichment Chamber de verdade guarda 20: a comparação vai até o tick em que
o rastro põe a primeira máquina, e o que a rede faz com consumidores (a divisão entre eles) é conferido nos cenários do `owner_trace.rs`, com o buffer do rastro.

O original é o rastro que o `EnergyNetworkTraceTest` gravou com o Mekanism de referência (`dev/referencia/rastros/energy_*.jsonl`, duas execuções): é o que o original fez, tick a tick.
"""
import json, os, subprocess, sys, time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
os.environ.setdefault("H_NAME", "horizon")
import h2_identity as h

RASTROS = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "rastros")
PASTA = os.environ.get("PASTA", "~/.cache/rust-mekanism/restos")
BASE = (0, 150, 0)
CABLE = {"basic": "mekanism:basic_universal_cable", "advanced": "mekanism:advanced_universal_cable", "elite": "mekanism:elite_universal_cable",
         "ultimate": "mekanism:ultimate_universal_cable"}
CHAMBER = "mekanism:enrichment_chamber"
SCENARIOS = ["tiers", "merge_full", "break_end_odd", "split_middle"]


def cmd(text):
    try:
        return h.command(text)[0]
    except (KeyError, IndexError):
        return ""


def wait(n):
    h.ctl("wait", str(n))


def at(dx, dy=0, dz=0):
    return (BASE[0] + dx, BASE[1] + dy, BASE[2] + dz)


def txt(p):
    return "%d %d %d" % p


def stand(p):
    cmd("tp @s %d %d %d" % p)
    wait(8)


def place(p, item):
    # O jogador precisa estar perto e com a face à vista: os cabos pelo norte da linha, as máquinas pelo sul.
    stand((p[0] + 2, p[1], p[2] - 2) if item in CABLE.values() else (p[0] + 2, p[1] + 1, p[2] + 2))
    h.ctl("give", item, "1")
    h.ctl("select", item)
    out = h.ctl("place", *map(str, p), "--item", item)
    if '"error"' in out:
        print("   place falhou:", out.strip()[:140])
    return '"error"' not in out


def clear():
    cmd("tick unfreeze")
    cmd("fill %d %d %d %d %d %d minecraft:air" % (BASE[0] - 4, BASE[1], BASE[2] - 6, BASE[0] + 40, BASE[1] + 6, BASE[2] + 8))
    wait(10)


def log_lines(prefix):
    out = subprocess.run(["ssh", "desktop", f"grep -a '^{prefix} ' {PASTA}/game.log | cut -c{len(prefix) + 2}-"], capture_output=True, text=True, timeout=120).stdout
    rows = []
    for line in out.splitlines():
        try:
            rows.append(json.loads(line))
        except ValueError:
            pass
    return rows


def step():
    """Um tick: devolve a linha `MBE` com `runs` verdadeiro que ele deu."""
    before = len(log_lines("MBE"))
    cmd("tick step 1")
    for _ in range(40):
        time.sleep(0.15)
        new = [row for row in log_lines("MBE")[before:] if row.get("runs")]
        if new:
            return new[-1]
    return None


def energy(p):
    out = cmd("data get block %s energy" % txt(p))
    try:
        return int(out.split()[-1].rstrip("b"))
    except (ValueError, IndexError):
        return 0


def rows_of(scenario, run=""):
    name = f"energy_{run + '_' if run else ''}{scenario}.jsonl"
    return [json.loads(l) for l in open(os.path.join(RASTROS, name)) if l.strip()]


def scenario_nets(rows):
    mine = set()
    for r in rows:
        if r["e"] == "state":
            mine |= {n["net"] for n in r["nets"]} | {c["net"] for c in r["cables"] if c and c.get("net") is not None}
    return mine


def run_energy(scenario, run=""):
    rows = rows_of(scenario, run)
    mine = scenario_nets(rows)
    last = max(r["t"] for r in rows)
    marks = {}
    for r in rows:
        if r["e"] == "mark":
            marks.setdefault(r["t"], []).append(r["m"])
    estates = {r["t"]: r for r in rows if r["e"] == "estate"}
    first_machine = min([r["t"] for r in rows if r["e"] == "mark" and r["m"].removeprefix("pre ").startswith("chamber")] or [last + 1])
    pushes = [r for r in rows if r["e"] == "push"]
    clear()
    cmd("gamemode creative @s")
    actors = {}
    state = {"pushed": 0}
    diffs = []

    def apply(mark):
        words = mark.removeprefix("pre ").split()
        n = lambda i: int(words[i])
        if words[0] == "place":
            place(at(n(1), 0, n(2)), CABLE[words[3]])
        elif words[0] == "break":
            cmd("setblock %s minecraft:air" % txt(at(n(1), 0, n(2))))
        elif words[0] == "chamber":
            p = at(n(2), n(3), n(4))
            # Sem o item não há a configuração de lados padrão (decisão 113): o `config0` põe `INPUT` nos seis lados, como o rastro (`input 63`).
            cmd("setblock %s %s" % (txt(p), CHAMBER))
            wait(5)
            cmd("data merge block %s {component_config:{config0:[I;1,1,1,1,1,1]},energy:%d}" % (txt(p), min(n(6), 2**31 - 1)))
            actors[words[1]] = p
        elif words[0] == "setenergy":
            cmd("data merge block %s {energy:%d}" % (txt(actors[words[1]]), n(2)))
        elif words[0] == "push":
            # O que um vizinho empurra pela capability do cabo: a caixa de entrada do Horizon (`energy` inteiro do block entity), que o dono passa para a rede.
            cmd("data merge block %s {energy:%d}" % (txt(at(n(1), 0, n(2))), n(3)))
            state["pushed"] += 1
        elif words[0] == "finish":
            pass
        else:
            raise SystemExit("ação desconhecida: " + mark)

    cmd("tick freeze")
    for t in range(0, last + 1):
        ms = marks.get(t, [])
        if any(m == "pre finish" for m in ms):
            break
        for m in ms:
            if m.startswith("pre "):
                apply(m)
        mbe = step()
        for m in ms:
            if not m.startswith("pre "):
                apply(m)
        want = estates.get(t)
        if want is None or t >= first_machine:
            continue
        want_nets = sorted((n["energy"], n["capacity"], n["prev"]) for n in want["nets"] if n["net"] in mine)
        got_nets = sorted((n[0], n[1], n[2]) for n in (mbe or {}).get("nets", []) if n[3] > 0)
        want_actors = {a["name"]: a["energy"] for a in want["actors"]}
        got_actors = {name: energy(p) for name, p in actors.items() if name in want_actors and want_actors[name] <= 1000}
        want_actors = {k: v for k, v in want_actors.items() if k in got_actors}
        if got_nets != want_nets or got_actors != want_actors:
            diffs.append((t, want_nets, got_nets, want_actors, got_actors))
    cmd("tick unfreeze")
    print(f"{run or 'run1'} {scenario}: {'IGUAL' if not diffs else 'DIFERE em %d ticks' % len(diffs)}")
    for d in diffs[:4]:
        print("   t=%d rastro %s %s  jogo %s %s" % (d[0], d[1], d[3], d[2], d[4]))
    return not diffs


def main():
    what = sys.argv[1] if len(sys.argv) > 1 else "energia"
    if what == "energia":
        names = SCENARIOS if len(sys.argv) < 3 or sys.argv[2] == "todos" else [sys.argv[2]]
        ok = True
        for name in names:
            ok &= run_energy(name)
        sys.exit(0 if ok else 1)
    raise SystemExit("uso: h2_cabo_jogo.py energia [CENARIO|todos]")


if __name__ == "__main__":
    main()
