#!/usr/bin/env python3
"""Repete em jogo os cenários de `TransmitterEventsTraceTest.java` (os eventos que a rede de transmissores recebe) e compara com o rastro do original.

    H_NAME=horizon PASTA=<pasta do game.log> h2_rede_ganchos.py basic|chunks|frozen|all [--out ARQUIVO.json]

A sonda (`TRANSMITTER_PROBE = true` na cópia de teste, `enviar_fontes.sh`) escreve `MBT {json}` no stderr do jogo, que `abrir_horizon.sh` põe em `PASTA/game.log`. O cenário roda como o
do Java (os cabos básicos, a Enrichment Chamber ao lado), cada operação marcada pela posição do log antes dela, e o script compara, operação por operação, a lista de eventos
(`clearRemoved`, `onAdded`, `neighbor`, `cap`, `setRemoved`, `onChunkUnloaded`) e a ordem dos tickers com `dev/referencia/rastros/transmitter_events_<cenário>.jsonl`.
Comparação: sem `pre`, `post`, `onLoad`, `ticket` e `tick` (a ordem dos tickers é conferida à parte), e sem repetição seguida do mesmo evento na mesma operação
(o `BlockCapabilityCache` do original só avisa de novo depois de um `getCapability`; a sonda do port entrega todos).
"""
import json, os, subprocess, sys, time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import h2_identity as h

RASTROS = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "rastros")
BASE = (0, 150, 0)
CABLE = "mekanism:basic_universal_cable"
CHAMBER = "mekanism:enrichment_chamber"
PASTA = os.environ.get("PASTA", "~/.cache/rust-mekanism/restos")
IGNORED = {"pre", "post", "onLoad", "ticket", "tick", "origin", "mark", "load", "unload"}


def abs_pos(rel):
    """A posição no jogo de uma posição relativa à estrutura do Java (o chão fica em y relativo 0)."""
    return (BASE[0] + rel[0], BASE[1] + rel[1] - 1, BASE[2] + rel[2])


def text(p):
    return f"{p[0]} {p[1]} {p[2]}"


def rel_of(p):
    return [p[0] - BASE[0], p[1] - BASE[1] + 1, p[2] - BASE[2]]


def log_lines():
    out = subprocess.run(["ssh", "desktop", f"grep '^MBT ' {PASTA}/game.log | cut -c5-"], capture_output=True, text=True, timeout=120).stdout
    events = []
    for line in out.splitlines():
        try:
            events.append(json.loads(line))
        except ValueError:
            pass  # a linha que o jogo ainda estava escrevendo
    return events


def chat(command):
    try:
        return h.command(command)[0]
    except (KeyError, IndexError):
        return ""


def wait_ticks(n):
    h.ctl("wait", str(n))


def prepare(width=7, depth=5, x0=BASE[0]):
    x, y, z = BASE
    h.ctl("respawn")
    h.ctl("close")
    chat("gamemode creative @s")
    chat("clear @s")
    chat("tick unfreeze")
    chat("tick unfreeze")
    chat(f"tp @s {x + 3} {y + 6} {z - 3}")
    wait_ticks(40)
    chat(f"fill {x - 4} {y - 1} {z - 6} {x + width + 4} {y - 1} {z + depth + 4} minecraft:stone")
    chat(f"fill {x - 4} {y} {z - 6} {x + width + 4} {y + 6} {z + depth + 4} minecraft:air")
    chat(f"tp @s {x + 3} {y} {z - 1}")
    wait_ticks(10)


def place(rel, item):
    p = abs_pos(rel)
    h.ctl("give", item, "1")
    h.ctl("select", item)
    out = json.loads(h.ctl("place", *map(str, p), "--item", item))
    if "error" in out:
        print("   place falhou:", out)
    return "error" not in out


def run_segments(ops):
    """Roda `ops` (rótulo, função) e devolve [(rótulo, eventos do log durante a operação e os ticks até a seguinte)]."""
    segments = []
    mark = len(log_lines())
    for label, action in ops:
        before = len(log_lines())
        action()
        wait_ticks(3)
        after = log_lines()
        segments.append((label, after[before:]))
    return segments


def normalize(events, origin_shift=None):
    """Os eventos comparáveis, na ordem, sem repetição seguida."""
    out = []
    for e in events:
        if e["e"] in IGNORED:
            continue
        item = (e["e"], tuple(rel_of(e["p"])) if "p" in e and origin_shift is None else tuple(e.get("p", [])), e.get("side"))
        if not out or out[-1] != item:
            out.append(item)
    return out


def java_segments(name):
    rows = [json.loads(line) for line in open(os.path.join(RASTROS, f"transmitter_events_{name}.jsonl"))]
    segments, current = [], None
    for row in rows:
        if row["e"] == "mark" and row["m"] not in ("start", "finish", "line built") and not row["m"].startswith(("border", "level ")):
            current = (row["m"], [])
            segments.append(current)
        elif current is not None and row["e"] not in IGNORED:
            item = (row["e"], tuple(row["p"]), row.get("side"))
            if not current[1] or current[1][-1] != item:
                current[1].append(item)
    return segments


def ticker_order(events):
    """Os cabos que ticam em cada passada, na ordem, a partir do primeiro tick de cada passada."""
    passes = {}
    for e in events:
        if e["e"] == "tick":
            passes.setdefault(e["t"], []).append(tuple(rel_of(e["p"])))
    return list(passes.values())


MACHINE_INTERNAL = ("place machine with the item",)


def unique(items):
    out = []
    for item in items:
        if item not in out:
            out.append(item)
    return out


def basic():
    prepare()
    ops = [
        ("place cable A with the item", lambda: place((3, 1, 2), CABLE)),
        ("place cable B with the item", lambda: place((2, 1, 2), CABLE)),
        ("place machine with the item", lambda: place((4, 1, 2), CHAMBER)),
        ("break machine", lambda: chat(f"setblock {text(abs_pos((4, 1, 2)))} minecraft:air destroy")),
        ("set machine with setBlock", lambda: chat(f"setblock {text(abs_pos((4, 1, 2)))} {CHAMBER}")),
        ("break cable B", lambda: chat(f"setblock {text(abs_pos((2, 1, 2)))} minecraft:air destroy")),
        ("set cable B with setBlock", lambda: chat(f"setblock {text(abs_pos((2, 1, 2)))} {CABLE}")),
        ("break cable A with setBlock air", lambda: chat(f"setblock {text(abs_pos((3, 1, 2)))} minecraft:air")),
    ]
    segments = run_segments(ops)
    return compare("basic", segments)


def compare(name, port_segments):
    if not os.path.exists(os.path.join(RASTROS, f"transmitter_events_{name}.jsonl")):
        for label, events in port_segments:
            print(f"[port] {label}: {normalize(events)}")
        return 0
    wrong = 0
    for (label, events), (_, expected) in zip(port_segments, java_segments(name)):
        mine = normalize(events)
        same = mine == expected
        note = ""
        if not same and label in MACHINE_INTERNAL and unique(mine) == unique(expected):
            # A Enrichment Chamber do original invalida a própria capability várias vezes ao ser posta (`invalidateCapabilitiesFull` de cada lado, o `onAdded`):
            # o port entrega uma. Mesmos eventos, sem as repetições da máquina (decisão 107).
            same, note = True, " (sem as invalidações repetidas da máquina)"
        wrong += not same
        print(f"[{'igual' if same else 'DIFERENTE'}] {label}{note}")
        if not same:
            print("   java:", expected)
            print("   port:", mine)
    print(f"{name}: {len(port_segments)} operações, {wrong} diferentes")
    return wrong


def line_events(events):
    """Os eventos da linha de cabos e os tickets dos chunks 0 e 1, na ordem, sem repetição seguida."""
    out = []
    for e in events:
        if e["e"] == "ticket" and e["cz"] == 0 and e["cx"] in (0, 1):
            item = ("ticket", (e["cx"], e["cz"]), e["loaded"])
        elif e["e"] in ("clearRemoved", "onChunkUnloaded", "setRemoved", "cap", "neighbor", "onAdded"):
            item = (e["e"], tuple(rel_of(e["p"])), e.get("side"))
        else:
            continue
        if not out or out[-1] != item:
            out.append(item)
    return out


def summarize(items):
    """Os eventos seguidos do mesmo tipo juntos: `clearRemoved x16 [x 16..31]`."""
    out, i = [], 0
    while i < len(items):
        j = i
        while j < len(items) and items[j][0] == items[i][0] and items[i][0] != "ticket":
            j += 1
        j = max(j, i + 1)
        group = items[i:j]
        xs = [g[1][0] for g in group if isinstance(g[1], tuple) and len(g[1]) == 3]
        out.append((items[i][0], len(group), (min(xs), max(xs)) if xs else items[i][1:]))
        i = j
    return out


def chunks():
    """O chunk da máquina sai (o jogador longe), volta, e sai o do cabo da borda, como `TransmitterEventsTraceTest.chunks`."""
    prepare(width=40, depth=4)
    x, y, z = BASE
    for i in range(0, 32):
        chat(f"setblock {x + i} {y} {z + 1} {CHAMBER if i == 16 else CABLE}")
    wait_ticks(10)
    result = {}
    for label, cx in (("unload", -20), ("load", 1), ("unload both", -22), ("load again", 1)):
        before = len(log_lines())
        chat(f"tp @s {cx * 16 + 8} {y} {z + 8}")
        wait_ticks(80)
        result[label] = log_lines()[before:]
        print(f"[port] {label}:")
        for entry in summarize(line_events(result[label])):
            print("   ", entry)
    return result


def run_flags(events):
    """O `runsNormally` de cada `post`, sem repetição seguida (contando quantas seguidas)."""
    out = []
    for e in events:
        if e["e"] == "post":
            if out and out[-1][0] == e["runs"]:
                out[-1][1] += 1
            else:
                out.append([e["runs"], 1])
    return out


def frozen():
    """O servidor parado e com passos: o `ServerTickEvent.Post` com `runsNormally` falso parado e verdadeiro nos dois passos, e os eventos de uma operação feita parado."""
    prepare()
    chat("gamemode creative @s")
    chat(f"setblock {text(abs_pos((3, 1, 2)))} {CABLE}")
    chat(f"setblock {text(abs_pos((4, 1, 2)))} {CHAMBER}")
    wait_ticks(8)
    start = len(log_lines())
    chat("tick freeze")
    time.sleep(0.5)
    before = len(log_lines())
    chat(f"setblock {text(abs_pos((2, 1, 2)))} {CABLE}")
    time.sleep(0.5)
    placed = log_lines()[before:]
    mid = len(log_lines())
    chat("tick step 2")
    time.sleep(0.35 + 0.05 * 2 + 0.6)
    stepped = log_lines()[mid:]
    chat(f"setblock {text(abs_pos((4, 1, 2)))} minecraft:air")
    time.sleep(0.5)
    chat("tick unfreeze")
    wait_ticks(5)
    events = log_lines()[start:]
    print("[port] cabo B posto parado:", normalize(placed))
    print("[port] passos:", run_flags(stepped), [(e["e"], rel_of(e["p"])) for e in stepped if e["e"] == "tick"])
    print("[port] runsNormally por trecho:", run_flags(events))
    flags = run_flags(events)
    # Parado: `post` com `runs` falso; os dois passos: dois `post` com `runs` verdadeiro entre parados (o Java: t=4 falso, t=9 e t=10 verdadeiro, t=11 falso).
    ok = any(not f for f, _ in flags) and any(f and n == 2 for f, n in flags[1:-1])
    print("frozen: parado=false, passos=true (2):", "ok" if ok else "DIFERENTE")
    return 0 if ok else 1


def check_unload(events):
    """Cada descarga: o ticket, as capabilities do chunk, todos os `onChunkUnloaded` e só depois os `setRemoved`, na mesma ordem."""
    wrong = 0
    batches = []
    i = 0
    while i < len(events):
        if events[i]["e"] == "onChunkUnloaded":
            j = i
            while j < len(events) and events[j]["e"] == "onChunkUnloaded":
                j += 1
            unloaded = [tuple(e["p"]) for e in events[i:j]]
            removed = [tuple(e["p"]) for e in events[j:] if e["e"] == "setRemoved"][: len(unloaded)]
            batches.append((unloaded, removed, events[j:j + 1 + 3 * len(unloaded)]))
            i = j
        else:
            i += 1
    for unloaded, removed, _ in batches:
        if unloaded != removed:
            wrong += 1
            print("   ordem diferente:", unloaded, removed)
    print(f"descargas: {len(batches)} lotes, {wrong} fora de ordem")
    return wrong


def main():
    which = sys.argv[1] if len(sys.argv) > 1 else "all"
    wrong = 0
    if which in ("basic", "all"):
        wrong += basic()
    if which in ("chunks", "all"):
        result = chunks()
        for label in ("unload", "unload both"):
            wrong += check_unload(result[label])
    if which in ("frozen", "all"):
        wrong += frozen()
    return 1 if wrong else 0


if __name__ == "__main__":
    sys.exit(main())
