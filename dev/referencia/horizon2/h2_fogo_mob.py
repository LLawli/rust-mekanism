#!/usr/bin/env python3
"""Em jogo (container `horizon`, H_NAME=horizon): o mob em chamas dentro de um fluido de mod e dentro da água do vanilla (frente `acabamento-conteudo`, decisão 18).

O original (`FluidType.canExtinguish` falso nos fluidos do Mekanism, `Fluid.entityInside` só apaga com `canFluidExtinguish`) deixa o mob queimar na coluna de `brine`; a água do vanilla o apaga.
Para cada fluido: uma coluna de 4 células dentro de pedra, um zumbi sem IA no fundo (de noite: o sol não o acende), `Fire:200s`, e o `Fire` e a vida lidos a cada meio segundo.

Uso: H_NAME=horizon h2_fogo_mob.py [brine|water ...]   (o jogo aberto num mundo plano com o jogador criativo)
"""
import json, os, subprocess, sys, time

TOOLS = os.path.dirname(os.path.abspath(__file__))
ORIGIN = (60, -60, 40)
LEITURAS = 6
INTERVALO = 0.5


def ctl(*args):
    return subprocess.run([f"{TOOLS}/ctl.sh", *args], capture_output=True, text=True, timeout=180).stdout


def cmd(text):
    try:
        return json.loads(ctl("command", text))["chat"]
    except (json.JSONDecodeError, KeyError):
        return []


def at(x, y, z):
    return f"{ORIGIN[0] + x} {ORIGIN[1] + y} {ORIGIN[2] + z}"


def numero(chat):
    texto = " ".join(chat) if isinstance(chat, list) else str(chat)
    achados = [t for t in texto.replace("s", " ").replace("f", " ").replace("d", " ").split() if t.lstrip("-").replace(".", "", 1).isdigit()]
    return float(achados[-1]) if achados else None


def uma(nome, bloco):
    cmd(f"fill {at(2, 0, 2)} {at(8, 8, 8)} minecraft:air")
    cmd(f"fill {at(3, 0, 3)} {at(7, 7, 7)} minecraft:stone")
    cmd(f"fill {at(5, 1, 5)} {at(5, 4, 5)} {bloco}")
    cmd("kill @e[type=minecraft:zombie]")
    cmd(f"summon minecraft:zombie {ORIGIN[0] + 5.5} {ORIGIN[1] + 1.0} {ORIGIN[2] + 5.5} {{NoAI:1b,Silent:1b,PersistenceRequired:1b,NoGravity:1b,Health:200f,Attributes:[{{id:\"minecraft:max_health\",base:200}}]}}")
    time.sleep(1.0)
    cmd("data merge entity @e[type=minecraft:zombie,limit=1] {Fire:200s}")
    leituras = []
    for _ in range(LEITURAS):
        time.sleep(INTERVALO)
        fogo = numero(cmd("data get entity @e[type=minecraft:zombie,limit=1] Fire"))
        vida = numero(cmd("data get entity @e[type=minecraft:zombie,limit=1] Health"))
        leituras.append((fogo, vida))
    print(nome, "fogo e vida a cada meio segundo:", leituras)
    cmd("kill @e[type=minecraft:zombie]")
    return leituras


def main():
    nomes = sys.argv[1:] or ["brine", "water"]
    blocos = {"brine": "mekanism:brine[level=0,up=true]", "water": "minecraft:water"}
    ctl("time", "18000")
    cmd("gamerule advance_time false")
    cmd("gamerule spawn_mobs false")
    cmd("tp @s " + at(5.5, 9, 9))
    resultado = {nome: uma(nome, blocos[nome]) for nome in nomes}
    queima = {nome: all(f is not None and f > 0 for f, _ in leituras) for nome, leituras in resultado.items()}
    print("segue queimando:", queima)


if __name__ == "__main__":
    main()
