#!/usr/bin/env python3
"""Repete no jogo, no container `horizon` (H_NAME=horizon), os rastros `infuser_const_*.jsonl` do Metallurgic Infuser com uma receita de
`per_tick_usage` (`ConstantUsageTraceTest.java`): a máquina posta por um jogador, com dois Heat Generators encostados, o servidor parado
(`tick freeze`) e as marcas do rastro aplicadas entre os ticks (`tick step`), como o teste em Java as aplica.

A receita é a de um data pack do mundo (`saves/<mundo>/datapacks/ic/`, o mesmo arquivo de `testes/dados/`), ligado antes de abrir a máquina.
A energia vem dos geradores e não é comparada (o rastro a mantém com um `refill`). Comparados: entrada, saída, tanque, progresso e
`used_so_far`, que o NBT do tile guarda. A cada marca, e dois ticks antes e depois, e a cada 10 ticks, o jogo é lido; entre dois pontos
andam `tick step N`.

Uso: H_NAME=horizon h2_infuser_const.py run|dry|reload|speed [...]   (o `reload` do rastro tem `reload`, que no jogo é salvar e reabrir:
fica a cargo de `reopen`, abaixo)
  reopen ETAPA   antes (monta, anda 90 ticks, anota o estado e salva) | depois (lê o estado do mundo reaberto e anda até o fim)
"""
import json, os, re, sys, time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import h2_identity as h

SPOT = (70, -60, 100)
GENERATORS = [(70, -60, 101), (70, -60, 99)]
TRACES = h.TRACES
STEP_EVERY = 10
AROUND = 2


def pos(p=SPOT):
    return " ".join(map(str, p))


def read():
    """O estado do tile como o rastro o grava: entrada, saída, tanque, progresso e `used_so_far`."""
    chat = h.command(f"data get block {pos()}")[0]
    items = {}
    for body in re.findall(r"\{count: (\d+), Slot: (\d+)b, id: \"(?:minecraft|mekanism):(\w+)\"\}", chat):
        items[int(body[1])] = [body[2], int(body[0])]
    tank = re.search(r"amount: (\d+)L, type: \"mekanism:(\w+)\"", chat)
    progress = re.search(r"progress: (\d+)", chat)
    used = re.search(r"used_so_far: (\d+)", chat)
    return {
        "input": items.get(1, ["", 0]),
        "output": items.get(2, ["", 0]),
        "tank": int(tank.group(1)) if tank else 0,
        "progress": int(progress.group(1)) if progress else 0,
        "used_so_far": int(used.group(1)) if used else 0,
    }


def expected(line):
    return {key: line[key] for key in ("input", "output", "tank", "progress", "used_so_far")}


def merge_items(**slots):
    """Reescreve a lista `Items` mantendo o que já está lá: cada argumento é `slotN=(id, quantidade)`, e quantidade 0 tira o slot."""
    chat = h.command(f"data get block {pos()}")[0]
    items = {}
    for body in re.findall(r"\{count: (\d+), Slot: (\d+)b, id: \"((?:minecraft|mekanism):\w+)\"\}", chat):
        items[int(body[1])] = (body[2], int(body[0]))
    for key, (id_, count) in slots.items():
        slot = int(key[4:])
        if count:
            items[slot] = (id_, count)
        else:
            items.pop(slot, None)
    body = ",".join(f'{{Slot:{slot}b,id:"{id_}",count:{count}}}' for slot, (id_, count) in sorted(items.items()))
    h.command(f"data merge block {pos()} {{Items:[{body}]}}")


def set_tank(chemical, amount):
    if amount == 0:
        h.command(f"data remove block {pos()} chemical_tanks")
    else:
        h.command(f'data merge block {pos()} {{chemical_tanks:[{{container:0b,stored:{{type:"mekanism:{chemical}",amount:{amount}L}}}}]}}')


def apply(mark):
    for action in mark.split(", "):
        words = action.split()
        if words[0] == "item":
            merge_items(slot1=(f"minecraft:{words[1]}", int(words[2])))
        elif words[0] == "tank":
            set_tank(words[1], int(words[2]))
        else:
            raise SystemExit(f"marca sem equivalente no jogo: {action}")


def fuel():
    for generator in GENERATORS:
        h.command(f'data merge block {pos(generator)} {{Items:[{{Slot:0b,id:"minecraft:coal",count:64}}]}}')


def build():
    h.command("tick unfreeze")
    h.ctl("close")
    h.command("difficulty peaceful")
    h.command("gamemode creative")
    h.ctl("tp", "70", "-60", "96")
    h.ctl("wait", "20")
    for p in [SPOT, *GENERATORS]:
        h.command(f"setblock {pos(p)} minecraft:air")
    time.sleep(0.5)
    h.command("clear @s")
    h.ctl("give", "mekanism:metallurgic_infuser", "1")
    out = json.loads(h.ctl("place", *map(str, SPOT), "--item", "mekanism:metallurgic_infuser"))
    assert "error" not in out, out
    for generator in GENERATORS:
        h.ctl("setblock", *map(str, generator), "mekanismgenerators:heat_generator")
    h.ctl("wait", "10")
    fuel()
    h.command("tick freeze")
    step(60)


def step(ticks):
    """`tick step N` volta antes de os N ticks andarem, e eles andam na taxa do servidor (20 por segundo): espera os 50 ms de cada um antes de
    mexer ou ler, senão o dado entra no meio dos ticks (medido: um `step 60` com meio segundo de folga deixou 12 ticks para depois da marca)."""
    h.command(f"tick step {ticks}")
    time.sleep(0.35 + 0.05 * ticks)


def checkpoints(lines, first):
    """As linhas a comparar: as marcas, dois ticks em volta, as trocas de saída e a cada STEP_EVERY."""
    points = set()
    for i, line in enumerate(lines):
        if i < first:
            continue
        if line["marks"]:
            points.update(range(max(first, i - AROUND), min(len(lines), i + AROUND + 1)))
        if i > 0 and (line["output"] != lines[i - 1]["output"] or line["tank"] != lines[i - 1]["tank"] and abs(line["tank"] - lines[i - 1]["tank"]) > 2):
            points.update(range(max(first, i - AROUND), min(len(lines), i + AROUND + 1)))
        if (i - first) % STEP_EVERY == 0:
            points.add(i)
    points.add(len(lines) - 1)
    return sorted(points)


def replay(scenario):
    lines = [json.loads(line) for line in open(f"{TRACES}/infuser_const_{scenario}.jsonl")]
    first = next(i for i, line in enumerate(lines) if line["marks"])
    build()
    points = checkpoints(lines, first)
    done = first
    bad = checked = 0
    for index in points:
        if index > done:
            # A marca de um ponto acontece depois do tick dele (e antes da amostra): anda até lá e só então aplica.
            step(index - done)
        done = index
        if index == first or lines[index]["marks"]:
            for mark in lines[index]["marks"]:
                apply(mark)
            time.sleep(0.15)
        if checked % 5 == 0:
            fuel()
        got, want = read(), expected(lines[index])
        checked += 1
        if got != want:
            bad += 1
            print(f"  DIFF {scenario} t={lines[index]['t']}: esperado {want} obtido {got}")
    # Os dois ticks seguintes ao salto de marcas em linhas que andam sem ser lidas já foram cobertos pelos pontos acima.
    print(("OK   " if not bad else "DIFF ") + f"{scenario}: {checked} pontos de {len(lines) - first} ticks, {bad} diferentes")
    return not bad


def reopen(stage):
    """Salvar e reabrir no meio da operação: `antes` monta e anota, `depois` lê o mundo reaberto."""
    saved = os.path.join(os.environ.get("SHOTS", "/tmp"), "reopen_state.json")
    if stage == "antes":
        build()
        merge_items(slot1=("minecraft:gravel", 3))
        set_tank("redstone", 1000)
        step(90)
        state = read()
        print("antes de salvar:", state)
        json.dump(state, open(saved, "w"))
    else:
        state = json.load(open(saved))
        h.command("tick freeze")
        got = read()
        print("depois de reabrir:", got)
        ok = got == state
        step(20)
        after = read()
        print("20 ticks depois:", after)
        ok &= after["progress"] == state["progress"] + 20 and after["tank"] == state["tank"] - 40 and after["used_so_far"] == state["used_so_far"] + 20
        step(200 - after["progress"] + 1)
        end = read()
        print("fim da operação:", end)
        ok &= end["output"] == ["flint", 1] and end["tank"] == 1000 - 400 and end["progress"] == 0 and end["used_so_far"] == 0
        print("OK   reabrir" if ok else "DIFF reabrir")
        sys.exit(0 if ok else 1)


def main():
    args = sys.argv[1:]
    if args and args[0] == "reopen":
        reopen(args[1])
        return
    ok = True
    for scenario in args or ["run", "dry"]:
        ok &= replay(scenario)
    h.command("tick unfreeze")
    print("TUDO IGUAL" if ok else "HÁ DIFERENÇAS")
    sys.exit(0 if ok else 1)


if __name__ == "__main__":
    main()
