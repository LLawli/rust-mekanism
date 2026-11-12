#!/usr/bin/env python3
"""Em jogo (container `horizon`, H_NAME=horizon): o jogador, o porco e o item solto numa coluna de 4 fontes de `brine`, tick a tick, contra o rastro do original
(`rastros/fluids_entity.jsonl`, `FluidsEntityTrace`; frente `fluido-entidade`, decisão 18).

O original mantém o jogador e o porco parados, sem gravidade, com o ar caindo 1 por tick; o item sobe devagar; o fogo não apaga. O servidor fica parado
(`tick freeze`) e cada leitura vem depois de um `tick step 1` (a espera é `0,35 + 0,05 N` s, ver `gotchas/teste-em-jogo-tick-step-e-reabrir-mundo`).

Uso: H_NAME=horizon h2_fluido_entidade.py [player|pig|item|air|fire|above|flow|water ...]   (o jogo aberto num mundo plano; o jogador sobrevive o tempo do teste)
Escreve `fluido_entidade.json` na pasta atual com as leituras.
"""
import json, os, re, subprocess, sys, time

TOOLS = os.path.dirname(os.path.abspath(__file__))
ORIGIN = (60, -60, 40)
TICKS = 30


def ctl(*args):
    return subprocess.run([f"{TOOLS}/ctl.sh", *args], capture_output=True, text=True, timeout=180).stdout


def cmd(text):
    try:
        return json.loads(ctl("command", text))["chat"]
    except (json.JSONDecodeError, KeyError):
        return []


def at(x, y, z):
    return f"{ORIGIN[0] + x} {ORIGIN[1] + y} {ORIGIN[2] + z}"


def step(n=1):
    cmd(f"tick step {n}")
    time.sleep(0.35 + 0.05 * n)


def numbers(chat):
    texto = " ".join(chat) if isinstance(chat, list) else str(chat)
    texto = texto.split("data:")[-1] if "data:" in texto else texto.split("entity data:")[-1]
    return [float(n) for n in re.findall(r"-?\d+\.?\d*(?:[eE]-?\d+)?", texto)]


def get(sel, path):
    return numbers(cmd(f"data get entity {sel} {path}"))


def column(block):
    cmd(f"fill {at(2, 0, 2)} {at(8, 12, 8)} minecraft:air")
    cmd(f"fill {at(3, 0, 3)} {at(7, 7, 7)} minecraft:stone")
    cmd(f"fill {at(5, 1, 5)} {at(5, 12, 5)} minecraft:air")
    cmd(f"fill {at(5, 1, 5)} {at(5, 4, 5)} {block}")


def read(sel, with_air=True):
    pos = get(sel, "Pos")
    motion = get(sel, "Motion")
    row = {"y": pos[1] if len(pos) > 1 else None, "dy": motion[1] if len(motion) > 1 else None}
    row["x"], row["z"] = pos[0], pos[2]
    if with_air:
        air = get(sel, "Air")
        row["air"] = air[-1] if air else None
    fall = get(sel, "fall_distance") or get(sel, "FallDistance")
    row["fall"] = fall[-1] if fall else None
    return row


def trace(sel, with_air=True, ticks=TICKS):
    rows = []
    for tick in range(1, ticks + 1):
        step(1)
        row = read(sel, with_air)
        row["tick"] = tick
        rows.append(row)
    return rows


def run(kind, block, start_y=1.0, ticks=TICKS):
    column(block)
    cmd("kill @e[type=minecraft:pig]")
    cmd("kill @e[type=minecraft:item]")
    x, y, z = ORIGIN[0] + 5.5, ORIGIN[1] + start_y, ORIGIN[2] + 5.5
    if kind == "player":
        cmd("gamemode survival @s")
        cmd(f"tp @s {x} {y} {z}")
        sel = "@s"
    elif kind == "pig":
        cmd(f"summon minecraft:pig {x} {y} {z} {{PersistenceRequired:1b,Silent:1b,Health:10f}}")
        sel = "@e[type=minecraft:pig,limit=1]"
    else:
        cmd(f"summon minecraft:item {x} {y} {z} {{Item:{{id:\"minecraft:stone\",count:1}},Age:-32768s,PickupDelay:32767s}}")
        sel = f"@e[type=minecraft:item,limit=1,x={x},y={y},z={z},distance=..40]"
    cmd("tick freeze")
    rows = trace(sel, with_air=kind != "item", ticks=ticks)
    cmd("tick unfreeze")
    cmd("kill @e[type=minecraft:pig]")
    cmd("kill @e[type=minecraft:item]")
    return rows


def gametime():
    texto = " ".join(cmd("time query gametime"))
    achados = re.findall(r"-?\d+", texto)
    return int(achados[-1]) if achados else None


def air_rate(block, seconds=3):
    """O ar do jogador com o jogo andando (o `tick step` mostra o ar do jogador errado: a leitura pula por passo): perda por tick de jogo."""
    column(block)
    cmd("gamemode survival @s")
    cmd(f"tp @s {ORIGIN[0] + 5.5} {ORIGIN[1] + 1.0} {ORIGIN[2] + 5.5}")
    cmd("tick unfreeze")
    time.sleep(0.5)
    t0, a0 = gametime(), get("@s", "Air")[-1]
    time.sleep(seconds)
    t1, a1 = gametime(), get("@s", "Air")[-1]
    cmd(f"tp @s {ORIGIN[0] + 5.5} {ORIGIN[1] + 14} {ORIGIN[2] + 9}")
    return {"ticks": t1 - t0, "air": a0 - a1, "per_tick": round((a0 - a1) / max(t1 - t0, 1), 3)}


def flow(ticks=20):
    """O porco e o jogador num corredor de fluido que corre (nível 3 a 5 vindos de uma fonte): ficam parados e a velocidade soma 0,014 por tick (`motionScale`)."""
    ox, oy, oz = ORIGIN[0] + 2, ORIGIN[1], ORIGIN[2] + 3
    cmd(f"fill {ox - 2} {oy - 1} {oz - 3} {ox + 14} {oy + 6} {oz + 3} minecraft:air")
    cmd(f"fill {ox} {oy - 1} {oz - 2} {ox + 12} {oy - 1} {oz + 2} minecraft:stone")
    cmd(f"fill {ox} {oy} {oz - 2} {ox + 12} {oy + 3} {oz - 2} minecraft:stone")
    cmd(f"fill {ox} {oy} {oz + 2} {ox + 12} {oy + 3} {oz + 2} minecraft:stone")
    cmd(f"setblock {ox} {oy} {oz} mekanism:brine[level=0,up=false]")
    time.sleep(6)
    out = {}
    for kind in ("pig", "player"):
        cmd("kill @e[type=minecraft:pig]")
        if kind == "pig":
            cmd(f"summon minecraft:pig {ox + 4.5} {oy} {oz + 0.5} {{PersistenceRequired:1b,Silent:1b}}")
            sel = f"@e[type=minecraft:pig,limit=1,x={ox + 4},y={oy},z={oz},distance=..20]"
        else:
            cmd("gamemode survival @s")
            cmd(f"tp @s {ox + 4.5} {oy} {oz + 0.5}")
            sel = "@s"
        cmd("tick freeze")
        rows = []
        for tick in range(1, ticks + 1):
            step(1)
            m, p = get(sel, "Motion"), get(sel, "Pos")
            rows.append({"tick": tick, "dx": m[0], "dz": m[2], "x": p[0], "z": p[2]})
        cmd("tick unfreeze")
        out[kind] = rows
    cmd("kill @e[type=minecraft:pig]")
    cmd(f"tp @s {ORIGIN[0] + 5.5} {ORIGIN[1] + 14} {ORIGIN[2] + 9}")
    cmd(f"fill {ox - 2} {oy - 1} {oz - 3} {ox + 14} {oy + 6} {oz + 3} minecraft:air")
    return out


def fire(block, who):
    """Em chamas dentro da coluna: `canExtinguish` falso nos fluidos do Mekanism deixa queimar; a água do vanilla apaga."""
    column(block)
    x, y, z = ORIGIN[0] + 5.5, ORIGIN[1] + 1.0, ORIGIN[2] + 5.5
    if who == "pig":
        cmd("kill @e[type=minecraft:pig]")
        cmd(f"summon minecraft:pig {x} {y} {z} {{PersistenceRequired:1b,Silent:1b,Health:100f,Attributes:[{{id:\"minecraft:max_health\",base:100}}]}}")
        cmd("data merge entity @e[type=minecraft:pig,limit=1] {Fire:200s}")
        sel = "@e[type=minecraft:pig,limit=1]"
    else:
        cmd("gamemode survival @s")
        # O jogador não aceita `data merge`: acende na pedra ao lado, em cima de um bloco de fogo, e entra na coluna.
        cmd(f"setblock {ORIGIN[0] + 9} {ORIGIN[1]} {ORIGIN[2] + 5} minecraft:lava")  # `ORIGIN[1]` é o nível do chão do `spike`; a lava acende o jogador (o fogo solto não pega nesse chão)
        cmd(f"tp @s {ORIGIN[0] + 9.5} {ORIGIN[1]} {ORIGIN[2] + 5.5}")
        time.sleep(1.0)
        cmd(f"setblock {ORIGIN[0] + 9} {ORIGIN[1]} {ORIGIN[2] + 5} minecraft:air")
        cmd(f"tp @s {x} {y} {z}")
        sel = "@s"
    out = []
    for _ in range(6):
        time.sleep(0.5)
        out.append((get(sel, "Fire") or [None])[-1])
    cmd("kill @e[type=minecraft:pig]")
    cmd(f"tp @s {ORIGIN[0] + 5.5} {ORIGIN[1] + 14} {ORIGIN[2] + 9}")
    return out


def main():
    wanted = sys.argv[1:] or ["player", "pig", "item", "fire"]
    ctl("respawn")  # o jogador que se afogou no teste anterior volta (sem efeito se está vivo)
    time.sleep(2)
    cmd("gamerule send_command_feedback true")
    cmd("weather clear")
    cmd("time set noon")
    cmd("gamerule advance_time false")
    cmd("gamerule advance_weather false")
    cmd("gamerule spawn_mobs false")
    cmd("tp @s " + at(5.5, 14, 9))
    result = {}
    for kind in ("player", "pig", "item"):
        if kind in wanted:
            result[kind] = run(kind, "mekanism:brine[level=0,up=true]")
    if "above" in wanted:
        trace_path = os.path.join(TOOLS, "..", "rastros", "fluids_entity.jsonl")
        original = [json.loads(l) for l in open(trace_path)]
        for kind in ("player", "pig", "item"):
            rows = run(kind, "mekanism:brine[level=0,up=true]", start_y=9.0, ticks=45)
            result[kind + "_above"] = rows
            ref = [r for r in original if r["env"] == "brine" and r["entity"] == kind and r["start"] == "above"]
            best = None
            for off in range(0, 4):
                diffs = [abs((row["y"] - ORIGIN[1]) - ref[min(row["tick"] + off, 40)]["y"]) for row in rows[: 40 - off]]
                if best is None or max(diffs) < best[1]:
                    best = (off, max(diffs))
            print(kind, "above: melhor desvio do y contra o rastro (deslocamento em ticks, maior diferença):", best)
    if "water" in wanted:
        for kind in ("player", "pig"):
            result[kind + "_water"] = run(kind, "minecraft:water")
    if "flow" in wanted:
        result.update({f"flow_{k}": v for k, v in flow().items()})
    if "air" in wanted:
        result["air_brine"] = air_rate("mekanism:brine[level=0,up=true]")
        result["air_water"] = air_rate("minecraft:water")
    if "fire" in wanted:
        for who in ("player", "pig"):
            result[f"fire_{who}_brine"] = fire("mekanism:brine[level=0,up=true]", who)
            result[f"fire_{who}_water"] = fire("minecraft:water", who)
    cmd(f"fill {at(2, 0, 2)} {at(8, 12, 8)} minecraft:air")
    cmd("gamemode creative @s")
    cmd("gamerule advance_time true")
    cmd("gamerule advance_weather true")
    cmd("gamerule spawn_mobs true")
    json.dump(result, open("fluido_entidade.json", "w"), indent=1)
    for name, rows in result.items():
        print(name)
        if isinstance(rows, dict):
            print("  ", rows)
        elif rows and isinstance(rows[0], dict):
            for r in rows[:: 5 if len(rows) > 12 else 1]:
                print("  ", {k: (round(v, 5) if isinstance(v, float) else v) for k, v in r.items()})
        else:
            print("  ", rows)


if __name__ == "__main__":
    main()
