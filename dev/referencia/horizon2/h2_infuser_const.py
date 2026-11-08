#!/usr/bin/env python3
"""Repete no jogo, no container `horizon` (H_NAME=horizon), os rastros `infuser_const_*.jsonl` do Metallurgic Infuser com uma receita de
`per_tick_usage` (`ConstantUsageTraceTest.java`): a máquina posta por um jogador, com dois Heat Generators encostados, o servidor parado
(`tick freeze`) e as marcas do rastro aplicadas entre os ticks (`tick step`), como o teste em Java as aplica.

A receita é a de um data pack do mundo (`saves/<mundo>/datapacks/ic/`, o mesmo arquivo de `testes/dados/`), ligado antes de abrir a máquina.
A energia vem dos geradores e não é comparada (o rastro a mantém com um `refill`). Comparados: entrada, saída, tanque, progresso e
`used_so_far`, que o NBT do tile guarda. A cada marca, e dois ticks antes e depois, e a cada 10 ticks, o jogo é lido; entre dois pontos
andam `tick step N`.

Uso: H_NAME=horizon h2_infuser_const.py run|dry|placed_speed|ops_placed [...]   (o `reload` do rastro tem `reload`, que no jogo é salvar e reabrir:
fica a cargo de `reopen`, abaixo)
  reopen ETAPA   antes (monta, anda 90 ticks, anota o estado e salva) | depois (lê o estado do mundo reaberto e anda até o fim)
"""
import json, os, re, subprocess, sys, time

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


def uninstall_one(kind):
    """O botão Uninstall da janela de upgrades (`PacketRemoveUpgrade`, um por clique): abre a tela, escolhe a linha do tipo e clica. O servidor segue parado; o pacote vale no tick seguinte."""
    h.ctl("interact", *map(str, SPOT))
    time.sleep(1.0)
    left = ((-(-1920 // GUI_SCALE)) - 176) // 2
    top = ((-(-1080 // GUI_SCALE)) - 166) // 2
    for x, y in (TAB, ROWS[kind], UNINSTALL):
        h.ctl("click-at", str(left + x), str(top + y))
        time.sleep(0.5)
    h.ctl("close")


def apply(mark):
    for action in mark.split(", "):
        words = action.split()
        if words[0] == "item":
            merge_items(slot1=(f"minecraft:{words[1]}", int(words[2])))
        elif words[0] == "tank":
            set_tank(words[1], int(words[2]))
        elif words[0] == "speed" and int(words[1]) == 7 and OPS_RUNNING:
            uninstall_one("speed")
        else:
            raise SystemExit(f"marca sem equivalente no jogo: {action}")


OPS_RUNNING = False


def set_multiplier(value):
    """`maxUpgradeMultiplier` no arquivo de configuração do servidor: o jogo o relê em até um segundo (`native_config_reloading`)."""
    script = f"f={CONFIG}; sed -i 's/^maxUpgradeMultiplier = .*/maxUpgradeMultiplier = {value}/' $f; grep ^maxUpgradeMultiplier $f"
    print("  config:", subprocess.run(["ssh", "desktop", script], capture_output=True, text=True).stdout.strip())
    time.sleep(3.0)


def fuel():
    for generator in GENERATORS:
        h.command(f'data merge block {pos(generator)} {{Items:[{{Slot:0b,id:"minecraft:coal",count:64}}]}}')


# Os cenários com upgrades: o item de bloco do jogo não traz `UPGRADES` (conteúdo guardado no item é da Fase 5), então a máquina é posta e os upgrades entram
# pelo jogador (o upgrade na mão, com shift, no bloco: `ItemUpgrade.useOn`), antes de o servidor parar. O rastro `infuser_const_placed_speed` compara
# entrada, saída, tanque, progresso e `used_so_far` (a energia vem dos geradores e não é comparada), que não dependem de como os upgrades chegaram.
UPGRADES = {"placed_speed": (4, 1), "ops_placed": (8, 2)}
TOPUP = 2300
# `ops_placed` é o rastro `infuser_const_ops_placed` (`maxUpgradeMultiplier` 1.000, 8 de velocidade e 2 de energia): passa de uma operação por tick, o consumo por tick é de seis
# dígitos e a energia é reposta a cada tick (`Trace.refill`), então ele anda de tick em tick. O multiplicador vem de `Mekanism/general.toml`, que o jogo relê sozinho. A marca `reload`
# (tile salvo e relido) não tem equivalente aqui, e a repetição para antes dela; `speed 7` é o botão Uninstall da janela de upgrades.
OPS = {"ops_placed": 1000}
CONFIG = "~/.local/share/jogo-isolado/horizon/data/horizon/config/Mekanism/general.toml"
# A janela de upgrades (veja `h2_som_medida.py`): a aba, a linha de cada tipo instalado (a energia vem antes da velocidade, na ordem do registro) e o botão Uninstall, em pixels de GUI
# relativos ao canto da tela de 176 por 166, com `guiScale` 3.
TAB, ROWS, UNINSTALL = (190, 19), {"energy": (29, 40), "speed": (29, 52)}, (132, 75)
GUI_SCALE = 3


def install_upgrades(speed, energy):
    """Pela janela de upgrades: a aba, o upgrade na mão, um clique no slot de entrada da janela e 25 ticks para instalar (20 de `UPGRADE_TICKS_REQUIRED`)."""
    h.ctl("tp", "70", "-60", "97")
    h.ctl("wait", "5")
    for _ in range(4):
        h.ctl("interact", *map(str, SPOT))
        time.sleep(0.6)
        if '"screen":"container"' in h.ctl("screen").replace(" ", ""):
            break
    h.ctl("click-at", str(232 + 190), str(97 + 19))
    time.sleep(0.7)
    scr = json.loads(h.ctl("screen"))
    first = sorted(s["slot"] for s in scr["slots"] if 300 <= s["slot"] < 500)[-2]
    for kind, count in (("speed", speed), ("energy", energy)):
        if not count:
            continue
        h.command("clear @s")
        h.command(f'item replace entity @s hotbar.8 with mekanism:upgrade[mekanism:upgrade_type="mekanism:{kind}"] {count}')
        time.sleep(0.4)
        h.ctl("click-slot", "8")
        h.ctl("click-slot", str(first))
        h.ctl("wait", "25")
    h.ctl("close")


def build(upgrades=None):
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
    if upgrades:
        install_upgrades(*upgrades)
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


def replay_ops(scenario):
    """De tick em tick, com a energia reposta ao fim de cada um (`Trace.refill`), até a marca `reload`."""
    global OPS_RUNNING
    lines = [json.loads(line) for line in open(f"{TRACES}/infuser_const_{scenario}.jsonl")]
    first = next(i for i, line in enumerate(lines) if line["marks"])
    stop = next(i for i, line in enumerate(lines) if any("reload" in mark for mark in line["marks"]))
    set_multiplier(OPS[scenario])
    OPS_RUNNING = True
    try:
        build(UPGRADES[scenario])
        done, bad, checked = first, 0, 0
        points = set(checkpoints(lines[:stop], first))
        for index in range(first, stop):
            if index > done:
                h.command(f"data merge block {pos()} {{energy:{lines[index - 1]['capacity']}}}")
                step(1)
            done = index
            if lines[index]["marks"]:
                for mark in lines[index]["marks"]:
                    apply(mark)
                time.sleep(0.15)
                h.command(f"data merge block {pos()} {{energy:{lines[index]['capacity']}}}")
            if index in points:
                got, want = read(), expected(lines[index])
                checked += 1
                if got != want:
                    bad += 1
                    print(f"  DIFF {scenario} t={lines[index]['t']}: esperado {want} obtido {got}")
        print(("OK   " if not bad else "DIFF ") + f"{scenario}: {checked} pontos de {stop - first} ticks (até a marca reload), {bad} diferentes")
        return not bad
    finally:
        OPS_RUNNING = False
        set_multiplier(10)


def replay(scenario):
    if scenario in OPS:
        return replay_ops(scenario)
    lines = [json.loads(line) for line in open(f"{TRACES}/infuser_const_{scenario}.jsonl")]
    first = next(i for i, line in enumerate(lines) if line["marks"])
    build(UPGRADES.get(scenario))
    points = checkpoints(lines, first)
    done = first
    bad = checked = 0
    for index in points:
        if index > done:
            # A marca de um ponto acontece depois do tick dele (e antes da amostra): anda até lá e só então aplica.
            if scenario in UPGRADES:
                # Com upgrades de velocidade o uso por tick passa do que dois geradores dão, e o rastro do Java mantém o cabo cheio (`Trace.refill`): a energia sobe
                # a cada passo (no máximo 10 ticks, que gastam bem menos do que isto).
                h.command(f"data merge block {pos()} {{energy:{TOPUP}}}")
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
