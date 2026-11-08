#!/usr/bin/env python3
"""Repete no jogo (container `horizon`, H_NAME=horizon) os rastros `heat_ambient_game_{desert,frozen}.jsonl` do Heat Generator em alturas
de baixo a perto do teto (`HeatAmbientTraceTest.java`): um gerador por altura, na mesma coluna, nas mesmas coordenadas do rastro, num
bioma só (`fillbiome` em volta de cada um). O servidor fica parado (`tick freeze`), o calor e a lava do primeiro tick do rastro vão
para o NBT do tile, e a cada `tick step 1` o jogo é lido e comparado com a linha do rastro: o calor (`heat`, o `double` igual ao bit,
que carrega o ar do bloco e o de cada face de todos os ticks) e a energia produzida (o buffer é zerado a cada tick, como o rastro faz).

`cache` repete `heat_ambient_cache.jsonl`: o bioma debaixo de dois geradores é trocado no meio (`fillbiome`) e o ar que eles leem não muda (o
`CachedAmbientTemperature` do objeto), até o tile ser recriado (o bloco sai e volta, e o calor do rastro volta ao tile).

Uso: H_NAME=horizon h2_temperatura.py desert|frozen|cache [ticks]
"""
import json, os, re, sys, time
from concurrent.futures import ThreadPoolExecutor

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import h2_identity as h

BLOCK = "mekanismgenerators:heat_generator"
RADIUS = 12
# O tanque do port guarda 1.000 (`TANK_CAPACITY`) e o rastro tem milhares: a cada tick o tanque volta ao que o rastro tem, até 1.000, e o
# que cada tick queima (100, `heatGenerationFluidRate`, ou nada quando o rastro já não tem lava) é o que se compara.
LAVA = 1000


def load(name):
    file = "heat_ambient_cache" if name == "cache" else f"heat_ambient_game_{name}"
    with open(f"{h.TRACES}/{file}.jsonl") as f:
        lines = [json.loads(line) for line in f]
    return lines[0], lines[1:]


def at(pos):
    return " ".join(map(str, pos))


def wait_loaded(x, z):
    for _ in range(60):
        out = h.command(f"setblock {x} 0 {z} minecraft:air")[0]
        if "not loaded" not in out:
            return
        time.sleep(1)
    raise SystemExit("o chunk não carregou")


def read(pos):
    chat = h.command(f"data get block {at(pos)}")[0]
    heat = re.search(r"heat: (-?[0-9.]+(?:E-?[0-9]+)?)d", chat)
    energy = re.search(r"energy: (-?\d+)", chat)
    amount = re.search(r"amount: (\d+)L", chat)
    if not heat or not energy:
        raise SystemExit(f"NBT inesperado em {pos}: {chat}")
    return float(heat.group(1)), int(energy.group(1)), int(amount.group(1)) if amount else 0


def tank(amount):
    """O tanque com a lava do rastro, até o que o tanque do port guarda."""
    amount = min(LAVA, amount)
    return f'tanks:[{{resource:"minecraft:lava",amount:{amount}L}}]' if amount else "tanks:[{amount:0L}]"


def refill(args):
    """O buffer vazio e o tanque como o rastro o tem para o tick seguinte (o rastro zera a energia a cada tick)."""
    pos, lava = args
    h.command(f"data merge block {at(pos)} {{energy:0,machinery_basis:{{{tank(lava)}}}}}")


def fill_biome(pos, biome):
    px, py, pz = pos
    lo, hi = max(py - RADIUS, -64), min(py + RADIUS, 319)
    return h.command(f"fillbiome {px - RADIUS} {lo} {pz - RADIUS} {px + RADIUS} {hi} {pz + RADIUS} {biome}")[0]


def recreate(pos, gens, lines, t):
    """O tile de `pos` é recriado (o objeto do Java que o chunk descarregado e carregado de novo faz nascer): o bloco sai e volta no mesmo
    tick, sem o `active_state` que o port usa para reconhecer o objeto velho, e um `tick step 1` o faz nascer. Esse tick anda todos os
    geradores, e por isso o calor, a energia e o tanque de todos voltam ao do rastro no tick `t`."""
    h.command(f"setblock {at(pos)} minecraft:air")
    h.command(f"setblock {at(pos)} {BLOCK}")
    h.command("tick step 1")
    time.sleep(0.5)
    for index, gen in enumerate(gens):
        h.command(
            f"data merge block {at(gen)} {{heat:{lines[t]['gens'][index]['heat']!r}d,energy:0,"
            f"machinery_basis:{{{tank(lines[t]['gens'][index]['lava'])}}}}}"
        )


def main():
    name = sys.argv[1]
    ticks = int(sys.argv[2]) if len(sys.argv) > 2 else 60
    header, lines = load(name)
    all_gens = header["gens"]
    gens = [g["pos"] for g in all_gens]
    # Dois geradores encostados trocam calor pela face (`simulateAdjacent`), que o port não troca (`lastTransferLoss` vale 0): ficam de
    # fora os que têm outro gerador do rastro encostado.
    keep = [i for i, p in enumerate(gens) if not any(sum(abs(a - b) for a, b in zip(p, q)) == 1 for q in gens)]
    assert len(keep) == len(gens) or name != "cache"
    gens = [gens[i] for i in keep]
    biomes = [all_gens[i]["biome"] for i in keep]
    lines = [{**line, "gens": [line["gens"][i] for i in keep]} for line in lines]
    h.command("gamemode creative")
    # Nada de uma rodada anterior segura um chunk carregado.
    h.command("forceload remove all")
    # O tile novo só nasce num tick: o servidor anda enquanto os blocos são postos.
    h.command("tick unfreeze")
    for px, py, pz in gens:
        h.command(f"tp @p {px} 8 {pz}")
        h.command(f"forceload add {px} {pz}")
        wait_loaded(px, pz)
    for pos, biome in zip(gens, biomes):
        print("fillbiome", pos[1], fill_biome(pos, biome))
    # Um tile de uma rodada anterior guarda o ar que leu (o cache do objeto): o bloco sai e o tile novo nasce no mesmo tick, sem o `active_state`
    # que faz o port reconhecer o objeto velho.
    for pos in gens:
        h.command(f"setblock {at(pos)} minecraft:air")
        h.command(f"setblock {at(pos)} {BLOCK}")
    time.sleep(3)
    h.command("tick freeze")
    first = lines[0]["gens"]
    for index, pos in enumerate(gens):
        h.command(
            f"data merge block {at(pos)} {{heat:{first[index]['heat_before']!r}d,energy:0,"
            f"machinery_basis:{{{tank(first[index]['lava'])}}}}}"
        )
    # O jogo não anda enquanto parado: o primeiro `tick step 1` é o tick 1 do rastro.
    bad = 0
    with ThreadPoolExecutor(min(len(gens), 4)) as pool:
        for t in range(1, ticks + 1):
            h.command("tick step 1")
            time.sleep(0.35)
            expected = lines[t]["gens"]
            got = list(pool.map(read, gens))
            for index, pos in enumerate(gens):
                heat, energy, lava = got[index]
                want = expected[index]
                before = lines[t - 1]["gens"][index]["lava"]
                burned = before - want["lava"]
                ok = heat == want["heat"] and energy == want["energy"] and min(LAVA, before) - lava == burned
                if not ok:
                    bad += 1
                    print(f"DIFF t={t} y={pos[1]}: calor {heat!r} (rastro {want['heat']!r}), energia {energy} ({want['energy']}), lava gasta {min(LAVA, before) - lava} (rastro {burned})")
            list(pool.map(refill, [(pos, expected[i]['lava']) for i, pos in enumerate(gens)]))
            for mark in lines[t]["marks"]:
                for action in mark.split(", "):
                    words = action.split(" ")
                    index = int(words[1])
                    if words[0] == "biome":
                        print(f"t={t} {action}:", fill_biome(gens[index], words[2]))
                    elif words[0] == "reload":
                        print(f"t={t} {action}")
                        recreate(gens[index], gens, lines, t)
            if t % 10 == 0:
                print(f"t={t} ok, {bad} diferenças até aqui", flush=True)
    # A leitura do último tick (calor, energia) de cada gerador, para a mídia: `H_DUMP=arquivo.json`.
    if os.environ.get("H_DUMP"):
        with open(os.environ["H_DUMP"], "w") as f:
            json.dump([{"y": pos[1], "heat": r[0], "energy": r[1]} for pos, r in zip(gens, got)], f)
    h.command("tick unfreeze")
    print("RESULTADO", name, f"{ticks} ticks, {len(gens)} geradores, {bad} diferenças")
    return bad


if __name__ == "__main__":
    sys.exit(1 if main() else 0)
