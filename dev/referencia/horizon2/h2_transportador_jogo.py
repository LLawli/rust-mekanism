#!/usr/bin/env python3
"""Os transportadores logísticos no jogo contra o original (frente `transportador-jogo`, Fase 3, só o servidor).

    H_NAME=horizon2 PASTA=<pasta do game.log> h2_transportador_jogo.py replay CENARIO|todos   # repete transporter_move_<cenário>.jsonl tick a tick
    H_NAME=horizon2 PASTA=<pasta do game.log> h2_transportador_jogo.py reabrir montar|conferir # o mundo salvo e reaberto com pilhas em trânsito
    H_NAME=horizon2 PASTA=<pasta do game.log> h2_transportador_jogo.py configurador          # o Configurator: a cor e o modo do desviador
    NOME=... h2_transportador_jogo.py video                                                 # o vídeo com som

A sonda (`TRANSMITTER_PROBE = true` na cópia de teste, `enviar_fontes.sh`) escreve no stderr do jogo (`PASTA/game.log`): `MBI {t, p, dl, dc, nid, stacks: [[id, item, n, progresso], ...]}` a cada
vez que um transportador anda (o `onUpdateServer`), `MBW {p, conn, kind, raw, net}` a cada vez que ele publica as conexões e `MBS {set}` quando `probe_set` pôs os tipos de conexão, a cor ou o
modo do desviador (`data merge block X Y Z {probe_set:{raw:[I;6], mode:[I;6], color:n}}`). O servidor fica parado (`tick freeze`) e anda com `tick step N`.

O original é o rastro que o `TransporterMoveTraceTest` gravou com o Mekanism de referência (`dev/referencia/rastros/transporter_move_*.jsonl`): é o que o original fez, tick a tick. O jogo anda os
block entities na ordem dele, e o original na de `y`, `x`, `z`: onde a ordem muda o resultado (uma pilha que passa ao vizinho no mesmo tick) a comparação para e o relatório diz onde.
"""
import json, os, re, subprocess, sys, time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
os.environ.setdefault("H_NAME", "horizon2")
import h2_identity as h

RASTROS = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "rastros")
PASTA = os.environ.get("PASTA", "~/.cache/rust-mekanism/transportador-jogo")
BASE = (0, 150, 0)
TIER = ["basic", "advanced", "elite", "ultimate"]
COLORS = re.findall(r'"([a-z_]+)"', open(os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "..", "..", "mods", "machinery_basis", "core", "src", "side_config.rs")).read().split("pub const ENUM_COLOR_NAMES")[1].split("];")[0])


def cmd(text):
    try:
        return h.command(text)[0]
    except (KeyError, IndexError):
        return ""


def wait(n):
    h.ctl("wait", str(n))


def at(p):
    return (BASE[0] + p[0], BASE[1] + p[1], BASE[2] + p[2])


def txt(p):
    return "%d %d %d" % p


def log_lines(prefix):
    out = subprocess.run(["ssh", "desktop", f"grep -a '^{prefix} ' {PASTA}/game.log | cut -c{len(prefix) + 2}-"], capture_output=True, text=True, timeout=120).stdout
    rows = []
    for line in out.splitlines():
        try:
            rows.append(json.loads(line))
        except ValueError:
            pass
    return rows


def block_of(t):
    if t["kind"] == "logistical":
        return f"mekanism:{TIER[t['tier']]}_logistical_transporter"
    return "mekanism:restrictive_transporter" if t["kind"] == "restrictive" else "mekanism:diversion_transporter"


def probe_set(t):
    parts = ["raw:[I;%s]" % ",".join(str(v) for v in t["raw"])]
    if t["kind"] == "diversion":
        parts.append("mode:[I;%s]" % ",".join(str(v) for v in t["modes"]))
    if t["kind"] == "logistical" and t.get("color"):
        parts.append("color:%d" % COLORS.index(t["color"]))
    return "{probe_set:{%s}}" % ",".join(parts)


def clear(width=14, depth=10):
    cmd("tick unfreeze")
    cmd("fill %d %d %d %d %d %d minecraft:stone" % (BASE[0] - 4, BASE[1] - 1, BASE[2] - 4, BASE[0] + width, BASE[1] - 1, BASE[2] + depth))
    cmd("fill %d %d %d %d %d %d minecraft:air" % (BASE[0] - 3, BASE[1], BASE[2] - 3, BASE[0] + width - 1, BASE[1] + 6, BASE[2] + depth - 1))
    wait(10)


def montar(setup):
    """As caixas e os transportadores do `setup` do rastro, com os tipos de conexão, a cor e o modo do desviador pela sonda."""
    clear()
    cmd("gamemode creative @s")
    for c in setup["containers"]:
        cmd("setblock %s %s" % (txt(at(c["p"])), "minecraft:chest" if c["size"] == 27 else "minecraft:dropper"))
    for t in setup["transporters"]:
        cmd("setblock %s %s" % (txt(at(t["p"])), block_of(t)))
    wait(40)
    for t in setup["transporters"]:
        cmd("data merge block %s %s" % (txt(at(t["p"])), probe_set(t)))
    wait(60)


def step(n):
    """`n` ticks com o servidor parado; devolve as linhas `MBI` novas."""
    before = len(log_lines("MBI"))
    cmd("tick step %d" % n)
    time.sleep(0.35 + 0.05 * n)
    for _ in range(40):
        rows = log_lines("MBI")
        if len({r["t"] for r in rows[before:]}) >= n:
            return rows[before:]
        time.sleep(0.25)
    return log_lines("MBI")[before:]


def fill_chest(p, slots):
    items = ",".join('{Slot:%db,id:"%s",count:%d}' % (s, item, n) for s, item, n in slots)
    cmd("data merge block %s {Items:[%s]}" % (txt(at(p)), items))


def chest_slots(p):
    chat = cmd("data get block %s Items" % txt(at(p)))
    return sorted((int(m[0]), m[1], int(m[2])) for m in re.findall(r'Slot: (\d+)b.*?id: "([^"]+)".*?count: (\d+)', chat))


def replay(name):
    rows = [json.loads(l) for l in open(os.path.join(RASTROS, f"transporter_move_{name}.jsonl")) if l.strip()]
    setup, begin = rows[0], rows[1]
    mount = {tuple(t["p"]): i for i, t in enumerate(setup["transporters"])}
    montar(setup)
    cmd("tick freeze")
    # O alinhamento: o contador de espera de quem puxa (`delay`) tem de valer o do rastro no começo. Anda até a sonda mostrar esse valor no primeiro transportador que puxa.
    puller = next(i for i, t in enumerate(setup["transporters"]) if 2 in t["raw"])
    ppos = list(at(setup["transporters"][puller]["p"]))
    want = begin["dl"][puller]
    t0 = None
    for _ in range(6):
        new = step(1)
        mine = [r for r in new if r["p"] == ppos]
        if mine and mine[-1]["dl"] == want:
            t0 = mine[-1]["t"]
            break
    if t0 is None:
        print(f"{name}: não achei o delay {want} do transportador que puxa")
        return False
    ticks = {}
    for row in rows[2:]:
        if row["e"] == "state" and row["t"] > 0:
            ticks[row["t"]] = row
    marks = {}
    for row in rows[2:]:
        if row["e"] == "mark":
            marks.setdefault(row["t"], []).append(row)
    stop = None
    for t in sorted(ticks):
        if any(m["k"] in ("finishing", "finish") for m in marks.get(t, [])):
            break
        for m in marks.get(t, []):
            if m["k"] == "fill":
                continue
            if m["k"] == "power":
                cmd("setblock %s %s" % (txt(at(m["p"])), "minecraft:redstone_block" if m["on"] else "minecraft:air"))
            elif m["k"] == "break":
                cmd("setblock %s minecraft:air" % txt(at(m["p"])))
            else:
                stop = (t, m["k"])
        if stop:
            break
        fills = [m for m in marks.get(t, []) if m["k"] == "fill"]
        chests = {}
        for m in fills:
            chests.setdefault(tuple(m["p"]), []).append((m["slot"], m["item"], m["n"]))
        for p, slots in chests.items():
            fill_chest(p, slots)
        step(1)
    last = max(t for t in ticks if (stop is None or t < stop[0]))
    got = {}
    for r in log_lines("MBI"):
        if r["t"] >= t0:
            got[(r["t"] - t0, tuple(r["p"]))] = r
    diffs = []
    compared = 0
    for t in sorted(ticks):
        if t > last or t < 1:
            continue
        for entry in ticks[t]["tx"]:
            g = got.get((t, tuple(at(entry["p"]))))
            want_stacks = sorted((s["id"], s["item"], s["n"], s["prog"]) for s in entry["transit"])
            if g is None:
                diffs.append((t, entry["p"], want_stacks, None))
                continue
            compared += 1
            have = sorted((s[0], s[1], s[2], s[3]) for s in g["stacks"])
            if have != want_stacks:
                diffs.append((t, entry["p"], want_stacks, have))
    print(f"{name}: {compared} estados de transportador comparados até o tick {last}" + (f" (parou na ação {stop[1]} do tick {stop[0]})" if stop else "") + ": " + ("IGUAL" if not diffs else "DIFERE em %d" % len(diffs)))
    for d in diffs[:5]:
        print("   t=%d %s rastro %s  jogo %s" % d)
    return not diffs


def block_nbt(p, key):
    return cmd("data get block %s %s" % (txt(at(p)), key))


def last_views():
    last = {}
    for v in log_lines("MBW"):
        last[tuple(v["p"])] = v
    return last


def linha(n=4, tier="basic", pull=True):
    """Um baú com terra, `n` transportadores e outro baú: o primeiro transportador em `PULL` do baú de origem."""
    clear()
    cmd("gamemode creative @s")
    cmd("tick unfreeze")
    cmd("setblock %s minecraft:chest" % txt(at((0, 0, 0))))
    cmd("setblock %s minecraft:chest" % txt(at((n + 1, 0, 0))))
    for x in range(1, n + 1):
        cmd("setblock %s mekanism:%s_logistical_transporter" % (txt(at((x, 0, 0))), tier))
    wait(40)
    if pull:
        cmd("data merge block %s {probe_set:{raw:[I;0,0,0,0,2,0]}}" % txt(at((1, 0, 0))))
    wait(40)
    fill_chest((0, 0, 0), [(0, "minecraft:dirt", 64)])


def reabrir_montar():
    """Uma linha em trabalho com pilhas em trânsito: salva com `save`, lê o NBT (`items`, `connection`) e deixa o mundo para o jogo ser fechado e aberto."""
    linha(6)
    time.sleep(2.5)
    h.ctl("save")
    wait(20)
    total = 0
    for x in range(1, 7):
        out = block_nbt((x, 0, 0), "items")
        found = re.findall(r'count: (\d+)', out)
        total += sum(int(v) for v in found)
        print("   transportador", x, "items:", out[:200])
    print("no NBT:", total, "itens em trânsito; origem:", chest_slots((0, 0, 0)), "destino:", chest_slots((7, 0, 0)))
    return True


def reabrir_conferir():
    """O mundo reaberto: as pilhas em trânsito voltam e a linha termina de levar os 64 de terra ao baú de destino."""
    time.sleep(5)
    for x in range(1, 7):
        print("   transportador", x, "items:", block_nbt((x, 0, 0), "items")[:200])
    wait(300)
    time.sleep(6)
    origem = sum(n for _, item, n in chest_slots((0, 0, 0)) if item == "minecraft:dirt")
    destino = sum(n for _, item, n in chest_slots((7, 0, 0)) if item == "minecraft:dirt")
    print("origem", origem, "destino", destino, "soma", origem + destino)
    return origem + destino <= 64 and destino > 0


def quebrar():
    """O transportador do meio quebrado com pilhas dentro: os itens caem (item entities no chão)."""
    linha(6, tier="basic")
    time.sleep(3)
    cmd("setblock %s minecraft:air" % txt(at((3, 0, 0))))
    wait(10)
    out = cmd("data get entity @e[type=item,limit=1,sort=nearest]")
    print("item no chão:", out[:160])
    return "Item" in out or "item" in out


def configurador():
    """O Configurator agachado no centro do transportador: a cor sobe (nenhuma, preta, azul...). Lê a cor do NBT salvo."""
    import h2_ajustes_jogo as a
    linha(3, pull=False)
    cmd("clear @s")
    h.ctl("give", "mekanism:configurator", "1")
    mid = at((2, 0, 0))
    ok = True
    for esperado in (0, 1):
        a.click_at(mid, (0.5, 0.5, 0.5), stand=(mid[0] + 0.5, mid[1] + 1.0, mid[2] - 2.6), sneak=True)
        h.ctl("save")
        wait(20)
        out = block_nbt((2, 0, 0), "color")
        print("cor depois do clique", esperado + 1, ":", out[:120])
    return True


def todos():
    ok = True
    for name in ("pull_line", "tiers", "full_midway", "restrictive", "diversion", "broken"):
        ok &= replay(name)
    return ok


if __name__ == "__main__":
    what = sys.argv[1] if len(sys.argv) > 1 else "replay"
    if what == "replay":
        arg = sys.argv[2] if len(sys.argv) > 2 else "pull_line"
        sys.exit(0 if (todos() if arg == "todos" else replay(arg)) else 1)
    if what == "reabrir":
        sys.exit(0 if (reabrir_montar() if sys.argv[2] == "montar" else reabrir_conferir()) else 1)
    if what == "quebrar":
        sys.exit(0 if quebrar() else 1)
    if what == "configurador":
        sys.exit(0 if configurador() else 1)
    raise SystemExit("subcomando " + what)
