#!/usr/bin/env python3
"""Em jogo (container `horizon`, H_NAME=horizon): o equipamento vestido do Mekanism desenhado no corpo (frente `armadura-vestida`), nas mesmas etapas que o original
(`testes/CenaEquip.java`, `cenas/equip_vestida.txt`): o jogador de pé, um armor stand a 2 blocos à direita e um husk a 2 à esquerda, as três figuras com as mesmas peças, dentro da mesma
caixa de concreto preto, com a mesma câmera. Os prints saem em `PASTA/equip_port_<nome>.png` (no desktop).

Uso: H_NAME=horizon h2_armadura_vestida.py CENA PASTA [NOME ...]   (CENA: o arquivo de etapas; sem NOME, todas as etapas)
O mundo precisa de uma área plana em volta de (70, -60, 50): o `spike`, copiado pelo `estado_horizon.sh`. O jogo precisa estar aberto e pronto (`wait-ready`)."""
import json, os, subprocess, sys, time

TOOLS = os.path.dirname(os.path.abspath(__file__))
X, Y, Z = 70.5, -60, 50.5
ESPERA = 3.0


def ctl(*args):
    out = subprocess.run([f"{TOOLS}/ctl.sh", *args], capture_output=True, text=True, timeout=180)
    return out.stdout


def cmd(text):
    ctl("command", text)


def etapas(arquivo):
    out = []
    for line in open(arquivo):
        line = line.strip()
        if not line or line.startswith("#"):
            continue
        if line.startswith("grupos"):
            continue
        campos = {"nome": "x", "pecas": "", "camera": "front", "entidades": "0", "giro": "0", "bebe": "0", "parede": "0", "encantado": "0"}
        for word in line.split()[1:]:
            k, _, v = word.partition("=")
            campos[k] = v
        campos["pecas"] = [p.split(":", 1) for p in campos["pecas"].split(",") if p]
        out.append(campos)
    return out


def camera(atual, alvo):
    """Aperta F5 até a câmera ser `alvo` (first, back, front): a volta é primeira pessoa, atrás, na frente."""
    ordem = ["first", "back", "front"]
    while atual != alvo:
        ctl("key", "f5", "tap")
        atual = ordem[(ordem.index(atual) + 1) % 3]
    return atual


ENCANTO = '[enchantments={"minecraft:unbreaking":1}]'


def uma(e, pasta, atual):
    inv = e["camera"] == "inv"
    encanto = ENCANTO if e["encantado"] == "1" else ""
    cmd("kill @e[type=!player]")
    cmd("clear @s")
    cmd(f"gamemode {'survival' if inv else 'creative'} @s")
    pitch = 60 if e["camera"] == "first" else 10
    cmd(f"tp @s {X} {Y} {Z} 0 {pitch}")
    for slot, item in e["pecas"]:
        cmd(f"item replace entity @s armor.{slot} with mekanism:{item}{encanto} 1")
    if e["parede"] == "1":
        x, y, z = int(X), int(Y), int(Z)
        wall = z + (3 if e["camera"] == "back" else -3)
        for outra in (z - 3, z + 3):
            cmd(f"fill {x - 10} {y} {outra} {x + 10} {y + 14} {outra} minecraft:air")
        cmd(f"fill {x - 10} {y - 1} {z - 6} {x + 10} {y - 1} {z + 6} minecraft:black_concrete")
        cmd(f"fill {x - 10} {y} {wall} {x + 10} {y + 14} {wall} minecraft:black_concrete")
    if e["entidades"] == "1":
        cmd(f"summon minecraft:armor_stand {X + 2} {Y} {Z} {{NoGravity:1b}}")
        baby = ",IsBaby:1b" if e["bebe"] == "1" else ""
        cmd(f"summon minecraft:husk {X - 2} {Y} {Z} {{NoAI:1b,Silent:1b,PersistenceRequired:1b,NoGravity:1b{baby}}}")
        time.sleep(1.0)
        for slot, item in e["pecas"]:
            cmd(f"item replace entity @e[type=minecraft:armor_stand,limit=1] armor.{slot} with mekanism:{item}{encanto} 1")
            cmd(f"item replace entity @e[type=minecraft:husk,limit=1] armor.{slot} with mekanism:{item}{encanto} 1")
        cmd(f"tp @e[type=minecraft:armor_stand,limit=1] {X + 2} {Y} {Z} {e['giro']} 0")
        cmd(f"tp @e[type=minecraft:husk,limit=1] {X - 2} {Y} {Z} {e['giro']} 0")
    atual = camera(atual, "first" if e["camera"] in ("first", "inv") else e["camera"])
    time.sleep(ESPERA)
    if inv:
        ctl("key", "e", "tap")
        time.sleep(1.5)
    ctl("screenshot", f"{pasta}/equip_port_{e['nome']}.png")
    time.sleep(1.0)
    if inv:
        ctl("close")
    return atual


def main():
    arquivo, pasta = sys.argv[1], sys.argv[2]
    nomes = set(sys.argv[3:])
    ctl("time", "6000")
    cmd("difficulty easy")
    cmd("weather clear")
    cmd("gamerule send_command_feedback false")
    # O jogo começa em primeira pessoa; o nome da câmera atual vem do `state` quando ele diz, senão se supõe primeira pessoa.
    atual = "first"
    try:
        state = json.loads(ctl("state"))
        atual = {"first_person": "first", "third_person_back": "back", "third_person_front": "front"}.get(str(state.get("camera", "")).lower(), "first")
    except (json.JSONDecodeError, AttributeError):
        pass
    for e in etapas(arquivo):
        if nomes and e["nome"] not in nomes:
            continue
        atual = uma(e, pasta, atual)
        print("print", e["nome"])
    atual = camera(atual, "first")
    cmd("kill @e[type=!player]")
    cmd("clear @s")


if __name__ == "__main__":
    main()
