#!/usr/bin/env python3
"""A troca do bloco vizinho de um transmissor, no jogo (frente `troca-vizinho`, Fase 4, decisões 222 a 224).

    H_NAME=horizon PASTA=<pasta do game.log> h2_troca_vizinho.py casos [NOME...]   # cada caso: o transmissor com o vizinho a leste, 8 ticks, a troca por `setblock`, 8 ticks; imprime a propriedade `east`
                                                                                    # do bloco e as linhas `MBS` (a sonda da troca, `enviar_fontes.sh WS PASTA troca`) de cada passo

O original de cada caso é `rastros/transmitter_swap.jsonl` (`testes/TransmitterSwapTraceTest.java`): o tipo cru e o visível do lado leste, `getAllCurrentConnections` e o fluxo.
O servidor fica parado (`tick freeze`) e anda com `tick step`."""
import json, os, re, subprocess, sys, time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
os.environ.setdefault("H_NAME", "horizon")
import h2_identity as h

PASTA = os.environ.get("PASTA", "~/.cache/rust-mekanism/restos")
BASE = (0, 150, 0)
PIPE = "mekanism:ultimate_mechanical_pipe"
CABLE = "mekanism:basic_universal_cable"
NEIGHBORS = {
    "air": "minecraft:air",
    "stone": "minecraft:stone",
    "chest": "minecraft:chest",
    "cauldron": "minecraft:cauldron",
    "lava_cauldron": "minecraft:lava_cauldron",
    "water_cauldron_1": "minecraft:water_cauldron[level=1]",
    "water_cauldron_3": "minecraft:water_cauldron[level=3]",
    "basic_energy_cube": "mekanism:basic_energy_cube",
    "ultimate_energy_cube": "mekanism:ultimate_energy_cube",
    "basic_fluid_tank": "mekanism:basic_fluid_tank",
    "ultimate_fluid_tank": "mekanism:ultimate_fluid_tank",
}
# O mesmo roteiro do rastro do original (`TransmitterSwapTraceTest.CASES`).
CASES = [
    ("cauldron_water_to_lava", PIPE, "pull", "water_cauldron_3", "lava_cauldron"),
    ("cauldron_water_to_lava_normal", PIPE, "normal", "water_cauldron_3", "lava_cauldron"),
    ("cauldron_empty_to_water", PIPE, "normal", "cauldron", "water_cauldron_3"),
    ("cauldron_water_3_to_1", PIPE, "normal", "water_cauldron_3", "water_cauldron_1"),
    ("cauldron_water_to_empty", PIPE, "normal", "water_cauldron_3", "cauldron"),
    ("cauldron_water_to_chest", PIPE, "normal", "water_cauldron_3", "chest"),
    ("chest_to_cauldron_water", PIPE, "normal", "chest", "water_cauldron_3"),
    ("stone_to_cauldron_water", PIPE, "normal", "stone", "water_cauldron_3"),
    ("cauldron_water_to_stone", PIPE, "normal", "water_cauldron_3", "stone"),
    ("cauldron_water_to_air", PIPE, "normal", "water_cauldron_3", "air"),
    ("air_to_cauldron_water", PIPE, "normal", "air", "water_cauldron_3"),
    ("cauldron_water_to_cauldron_pull_chest", PIPE, "pull", "water_cauldron_3", "chest"),
    ("tank_basic_to_ultimate", PIPE, "normal", "basic_fluid_tank", "ultimate_fluid_tank"),
    ("cauldron_water_to_tank", PIPE, "normal", "water_cauldron_3", "basic_fluid_tank"),
    ("cube_basic_to_ultimate", CABLE, "normal", "basic_energy_cube", "ultimate_energy_cube"),
    ("stone_to_cube", CABLE, "normal", "stone", "basic_energy_cube"),
    ("cube_to_stone", CABLE, "normal", "basic_energy_cube", "stone"),
    ("cube_to_chest", CABLE, "normal", "basic_energy_cube", "chest"),
    ("chest_to_cube", CABLE, "normal", "chest", "basic_energy_cube"),
    ("cube_to_air", CABLE, "normal", "basic_energy_cube", "air"),
]


def cmd(text):
    try:
        return h.command(text)[0]
    except (KeyError, IndexError):
        return ""


def step(n):
    cmd(f"tick step {n}")
    h.ctl("wait", "3")


def log_lines():
    out = subprocess.run(["ssh", "desktop", f"wc -l < {PASTA}/game.log"], capture_output=True, text=True)
    return int(out.stdout.strip() or 0)


def log_since(offset):
    out = subprocess.run(["ssh", "desktop", f"tail -n +{offset + 1} {PASTA}/game.log | grep '^MBS'"], capture_output=True, text=True)
    return [line for line in out.stdout.splitlines() if line]


def state_at(p):
    raw = h.ctl("block", str(p[0]), str(p[1]), str(p[2]))
    try:
        return json.loads(raw).get("state", "?")
    except ValueError:
        return raw[:100]


def arm(state):
    match = re.search(r"east=(\w+)", state)
    return match.group(1) if match else "-"


def run_case(case):
    name, transmitter, mode, before, after = case
    x, y, z = BASE
    cmd("tick unfreeze")
    cmd(f"fill {x - 3} {y} {z - 3} {x + 4} {y + 3} {z + 3} minecraft:air")
    cmd(f"fill {x - 3} {y - 1} {z - 3} {x + 4} {y - 1} {z + 3} minecraft:stone")
    step(2)
    cmd("tick freeze")
    cmd(f"setblock {x + 1} {y} {z} {NEIGHBORS[before]}")
    cmd(f"setblock {x} {y} {z} {transmitter}")
    if mode == "pull":
        # O Configurator agachado cicla o tipo do lado leste (NORMAL, PUSH, PULL): dois cliques.
        pass
    step(8)
    offset = log_lines()
    s1 = state_at((x, y, z))
    cmd(f"setblock {x + 1} {y} {z} {NEIGHBORS[after]}")
    step(8)
    s2 = state_at((x, y, z))
    lines = log_since(offset)
    print(f"{name}: {before} -> {after}: east {arm(s1)} -> {arm(s2)}")
    for line in lines:
        print("   ", line)


GRAVADOR = "~/.cache/rust-mekanism/gravar-jogo"
VX, VY, VZ = 62, -60, 82
SNEAK_EYE = 1.27
GAMERULES = ("advance_time", "advance_weather")


def aim(feet, target):
    """O `yaw` e o `pitch` que levam o olho (agachado) de quem está em `feet` ao ponto `target`: o jogador fica num ponto e num ângulo fixos, sem `look-at` (que fazia a câmera derivar)."""
    import math
    eye = (feet[0], feet[1] + SNEAK_EYE, feet[2])
    dx, dy, dz = target[0] - eye[0], target[1] - eye[1], target[2] - eye[2]
    yaw = math.degrees(math.atan2(-dx, dz))
    pitch = -math.degrees(math.atan2(dy, math.hypot(dx, dz)))
    return yaw, pitch


def video(out, seconds, container):
    """O Configurator troca o tipo do braço leste de um cabo sozinho que dá num cubo de energia (`PUSH`, `PULL`, `NONE`, `NORMAL`) e o modelo muda. O jogador fica num ponto e num ângulo fixos
    (`/tp` com o yaw e o pitch calculados até o braço, 45 graus de lado e de cima, sem `look-at`), agachado, e o `/tp` se repete antes de cada clique."""
    import h2_ajustes_jogo as aj
    import h2_blocos as hb
    cmd("weather clear")
    cmd("time set noon")
    for rule in GAMERULES:
        cmd(f"gamerule {rule} false")
    cmd("tick unfreeze")
    cmd("gamemode creative")
    cmd(f"fill {VX - 4} {VY} {VZ - 6} {VX + 8} {VY + 4} {VZ + 4} minecraft:air")
    cmd(f"setblock {VX} {VY} {VZ - 2} minecraft:stone")
    cmd(f"setblock {VX + 1} {VY} {VZ} mekanism:basic_energy_cube")
    h.ctl("wait", "20")
    cmd(hb.comando_do_cubo(f"#cubo {VX + 1} {VY} {VZ} 0.5 front=input left=input right=input back=input top=input bottom=input", [f"setblock {VX + 1} {VY} {VZ} mekanism:basic_energy_cube"]))
    h.ctl("give", "mekanism:configurator", "1")
    h.ctl("give", "mekanism:basic_universal_cable", "4")
    feet = (VX + 0.85, VY + 1.0, VZ - 2 + 0.73)
    target = (VX + aj.ARM["east"][0], VY + 0.5, VZ + 0.5)
    yaw, pitch = aim(feet, target)
    pose = "tp @s %.3f %.3f %.3f %.3f %.3f" % (feet + (yaw, pitch))
    print("pose", pose)
    cmd(pose)
    time.sleep(3.0)
    # O som do jogo só sobe com o stream num sink de teste (`audio_nulo.sh seguro`): o `rec` grava o som do container.
    seguro = subprocess.run(["ssh", "desktop", "~/.cache/rust-mekanism/audio_nulo.sh seguro " + (container or "horizon")], capture_output=True, text=True)
    if not seguro.stdout.startswith("ok"):
        raise SystemExit("som inseguro: " + seguro.stdout)
    for option in ("master", "block", "player"):
        h.ctl("options-set", f"soundCategory_{option}", "1.0")
    gravar = f"{GRAVADOR} rec troca-vizinho {out} {seconds}" + (f" {container}" if container else "")
    proc = subprocess.Popen(["ssh", "desktop", gravar])
    time.sleep(3.0)
    # O jogador põe o cabo no chão (o som do bloco posto).
    h.ctl("select", "mekanism:basic_universal_cable")
    h.ctl("look-at", "%.2f" % (VX + 0.5), "%.2f" % VY, "%.2f" % (VZ + 0.5))
    time.sleep(1.0)
    h.ctl("mouse", "right", "click")
    time.sleep(2.5)
    h.ctl("select", "mekanism:configurator")
    cmd(pose)
    time.sleep(1.5)
    h.ctl("key", "shift", "down")
    time.sleep(1.5)
    for _ in range(3):
        cmd(pose)
        time.sleep(0.6)
        h.ctl("mouse", "right", "click")
        time.sleep(3.0)
    h.ctl("key", "shift", "up")
    proc.wait()
    for option in ("master", "block", "player"):
        h.ctl("options-set", f"soundCategory_{option}", "0.0")


def devolver():
    for rule in GAMERULES:
        cmd(f"gamerule {rule} true")
    cmd(f"fill {VX - 4} {VY} {VZ - 6} {VX + 8} {VY + 4} {VZ + 4} minecraft:air")


if __name__ == "__main__":
    import time
    if sys.argv[1] == "video":
        video(sys.argv[2], int(sys.argv[3]) if len(sys.argv) > 3 else 24, sys.argv[4] if len(sys.argv) > 4 else "")
        sys.exit(0)
    if sys.argv[1] == "devolver":
        devolver()
        sys.exit(0)
    if sys.argv[1] == "casos":
        wanted = sys.argv[2:]
        for case in CASES:
            if not wanted or case[0] in wanted:
                run_case(case)
        cmd("tick unfreeze")
