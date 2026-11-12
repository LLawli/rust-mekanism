#!/usr/bin/env python3
"""Vídeo em jogo (container `horizon`, H_NAME=horizon): o jogador, três porcos e um item numa piscina de salmoura (frente `fluido-entidade`, decisão 18).

O original mantém o jogador e o porco parados na superfície do fluido de mod (sem gravidade, sem nadar), e o item flutua devagar. O script monta a piscina, devolve as
`gamerule` ao fim, e grava a cena com `~/.cache/rust-mekanism/gravar-jogo rec` (o gamescope já com o PipeWire privado, `procedures/gravar-video-real-do-jogo`).

Uso: H_NAME=horizon h2_fluido_entidade_video.py [segundos]
"""
import json, os, subprocess, sys, threading, time

TOOLS = os.path.dirname(os.path.abspath(__file__))
ORIGIN = (60, -60, 40)


def ctl(*args):
    return subprocess.run([f"{TOOLS}/ctl.sh", *args], capture_output=True, text=True, timeout=180).stdout


def cmd(text):
    try:
        return json.loads(ctl("command", text))["chat"]
    except (json.JSONDecodeError, KeyError):
        return []


def at(x, y, z):
    return f"{ORIGIN[0] + x} {ORIGIN[1] + y} {ORIGIN[2] + z}"


def main():
    seconds = int(sys.argv[1]) if len(sys.argv) > 1 else 30
    ctl("respawn")
    time.sleep(2)
    for g in ("weather clear", "time set noon", "gamerule advance_time false", "gamerule advance_weather false", "gamerule spawn_mobs false", "gamemode survival @s"):
        cmd(g)
    cmd(f"fill {at(0, 0, 0)} {at(14, 12, 14)} minecraft:air")
    cmd(f"fill {at(3, 0, 3)} {at(9, 3, 9)} minecraft:stone")
    cmd(f"fill {at(4, 0, 4)} {at(8, 3, 8)} mekanism:brine[level=0,up=true]")
    cmd(f"tp @s {at(6.5, 4, 3.5)} 0 20")  # em cima do muro, de frente para a piscina
    time.sleep(3)
    cmd("kill @e[type=minecraft:pig]")
    cmd("kill @e[type=minecraft:item]")
    ctl("key", "f5", "tap")  # terceira pessoa
    time.sleep(1.5)
    rec = threading.Thread(target=lambda: subprocess.run(["ssh", "desktop", f"~/.cache/rust-mekanism/gravar-jogo rec fluido-entidade cena.mp4 {seconds}"], capture_output=True))
    rec.start()
    time.sleep(3)
    for i, dx in enumerate((-1, 0, 1)):
        cmd(f"summon minecraft:pig {at(6.5 + dx, 8 + 2 * i, 6.5)} {{PersistenceRequired:1b,Silent:1b}}")
        time.sleep(1.5)
    cmd(f"summon minecraft:item {at(6.5, 1.0, 5.5)} {{Item:{{id:\"minecraft:gold_ingot\",count:1}},PickupDelay:32767s}}")
    time.sleep(5)
    ctl("move", "forward", "3")  # o jogador anda para a piscina e fica parado na superfície
    time.sleep(6)
    ctl("turn", "180", "0")
    time.sleep(6)
    rec.join()
    ctl("key", "f5", "tap")
    ctl("key", "f5", "tap")
    cmd("kill @e[type=minecraft:pig]")
    cmd("kill @e[type=minecraft:item]")
    cmd(f"fill {at(0, 0, 0)} {at(14, 12, 14)} minecraft:air")
    for g in ("gamerule advance_time true", "gamerule advance_weather true", "gamerule spawn_mobs true", "gamemode creative @s"):
        cmd(g)


if __name__ == "__main__":
    main()
