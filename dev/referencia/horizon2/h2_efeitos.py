#!/usr/bin/env python3
"""Em jogo (`H_NAME=horizon` ou `horizon2`): os efeitos de tela do Configurator (decisão 48) nas mesmas etapas que o original (`testes/CenaConfigurator.java`,
`cenas/configurator.txt`): a sobreposição da face, o texto do modo sobre a barra, o texto do HUD e o menu radial. Os prints saem em `PASTA/configurator_port_<nome>.png` (no desktop).

Uso: H_NAME=horizon h2_efeitos.py CENA PASTA [NOME ...]   (CENA: o arquivo de etapas; sem NOME, todas as etapas)
O mundo precisa de uma área plana em volta de (300, -60, 0) (o `spike`, copiado pelo `estado_horizon.sh`, tem a de (70, -60, 64): a cena abre o seu chão). O jogo precisa estar
aberto e pronto (`wait-ready`), em primeira pessoa."""
import json, os, subprocess, sys, time

TOOLS = os.path.dirname(os.path.abspath(__file__))
X, Y, Z = 300, -61, 0
MODOS = ["configurate_items", "configurate_fluids", "configurate_chemicals", "configurate_energy", "configurate_heat", "empty", "rotate", "wrench"]
GUI = (480, 270)


def ctl(*args):
    out = subprocess.run([f"{TOOLS}/ctl.sh", *args], capture_output=True, text=True, timeout=180)
    return out.stdout


def cmd(text):
    return ctl("command", text)


def etapas(arquivo):
    out = []
    for line in open(arquivo):
        line = line.strip()
        if not line or line.startswith("#"):
            continue
        campos = {}
        for word in line.split():
            k, _, v = word.partition("=")
            campos[k] = v
        out.append(campos)
    return out


def montar(e):
    cmd(f"gamemode {'survival' if e.get('sobrevivencia') == '1' else 'creative'} @s")
    cmd("clear @s")
    cmd(f"fill {X - 4} {Y} -2 {X + 4} {Y + 5} 2 minecraft:air")
    modo = e.get("modo", "configurate_items")
    # O texto do modo sobre a barra nasce de uma troca de modo (a rolagem agachado): a pilha começa no modo de antes.
    inicial = MODOS[(MODOS.index(modo) - 1) % len(MODOS)] if e.get("efeito") == "status" else modo
    cmd(f'item replace entity @s hotbar.0 with mekanism:configurator[mekanism:configurator_mode="{inicial}"] 1')
    ctl("select", "1")
    if e.get("efeito") == "overlay":
        cmd(f"setblock {X} {Y} {Z} mekanism:enrichment_chamber[facing=north]")
        time.sleep(0.5)
        cmd(f"data merge block {X} {Y} {Z} {{component_config:{{config3:[I;{e.get('config', '1,4,7,3,5,9')}],config0:[I;0,0,0,0,0,0]}}}}")
    x, y, z, yaw, pitch = e.get("tp", "300.5,-60,3.5,180,10").split(",")
    cmd(f"tp @s {x} {y} {z} {yaw} {pitch}")


def uma(e, pasta):
    montar(e)
    ctl("wait", "40")
    efeito = e.get("efeito")
    espera = int(e.get("espera", "60"))
    if efeito == "status":
        # `CenaConfigurator` arma o contador no tique 10 e tira o print no tique `espera`.
        ctl("key", "shift", "down")
        ctl("scroll-at", "240", "135", "1")
        ctl("key", "shift", "up")
        ctl("wait", str(max(espera - 10, 1)))
    elif efeito == "radial":
        ctl("key", "n", "down")
        ctl("wait", "10")
        dx, dy = (float(v) for v in e.get("mouse", "0,0").split(","))
        ctl("scroll-at", str(round(GUI[0] / 2 + dx)), str(round(GUI[1] / 2 + dy)), "0")
        ctl("wait", "5")
    else:
        ctl("wait", "20")
    ctl("screenshot", f"{pasta}/configurator_port_{e['nome']}.png")
    time.sleep(1.0)
    if efeito == "radial":
        ctl("key", "n", "up")
        ctl("wait", "10")


def main():
    arquivo, pasta = sys.argv[1], sys.argv[2]
    nomes = set(sys.argv[3:])
    cmd("weather clear")
    ctl("time", "6000")  # meio-dia
    cmd("gamerule advance_time false")
    cmd("gamerule advance_weather false")
    ctl("options-set", "chatVisibility", "2")
    # O original roda a 1920 por 1080 com a escala de GUI 4 (480 por 270 pixels de GUI).
    ctl("options-set", "guiScale", "4")
    cmd("difficulty peaceful")
    cmd("weather clear")
    cmd("gamerule send_command_feedback false")
    cmd(f"fill {X - 8} {Y - 1} -8 {X + 8} {Y + 20} 12 minecraft:air")
    cmd(f"fill {X - 8} {Y - 1} -8 {X + 8} {Y - 1} 12 minecraft:grass_block")
    cmd(f"fill {X - 8} {Y} -3 {X + 8} {Y + 10} -3 minecraft:black_concrete")
    cmd(f"fill {X - 4} {Y} -2 {X + 4} {Y + 5} 2 minecraft:air")
    for e in etapas(arquivo):
        if nomes and e["nome"] not in nomes:
            continue
        uma(e, pasta)
        print("print", e["nome"])
    cmd("clear @s")


if __name__ == "__main__":
    main()
