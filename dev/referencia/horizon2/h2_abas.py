#!/usr/bin/env python3
"""Em jogo, no container `horizon` (H_NAME=horizon): o clique com terra no cursor sobre as abas laterais das quatro máquinas do Mekanism
(`GuiMekanism.hasClickedOutside`, `GuiMekanism.java:397-399`). Para cada máquina, nos quatro estados (sem e com o upgrade de xp instalado, sem e com
aviso, que aqui é a falta de energia com entrada no slot): a aba de energia, a de avisos e o botão da de xp mantêm a terra no cursor quando o original as
mostra (ativas e visíveis), e soltam a terra quando a aba não existe naquele estado; a borda do suporte da aba de xp (que não é elemento: só o botão de dentro
de 18 por 18 conta) e a margem longe de tudo soltam, como no original. Compara com `rastros/fora.jsonl` (o cliente oficial): o que o rastro diz "sobre elemento"
é o que deve manter o item.

Uso: H_NAME=horizon TAG=depois h2_abas.py [máquina ...]   (sem argumentos, as quatro; `TAG` nomeia os prints em SHOTS, `antes` ou `depois`)
Os prints (um por máquina e estado, com o cursor sobre a aba de avisos ou a de energia) saem em SHOTS (padrão: .../horizon/data/shots/abas).
O canto da tela de 176 por 166 está em (232, 97) com guiScale 3 numa janela de 1920 por 1080.
"""
import json, os, subprocess, sys, time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import h2_armor as a
import h2_identity as h

MACHINES = {
    "enrichment_chamber": ("mekanism:enrichment_chamber", "coal", 0),
    "crusher": ("mekanism:crusher", "blaze_rod", 0),
    "energized_smelter": ("mekanism:energized_smelter", "raw_iron", 0),
    "metallurgic_infuser": ("mekanism:metallurgic_infuser", "clay_ball", 1),
}
SPOT = (-27, 74, -4)
STAND = "-26.5 74 -1.5 180 20"
SHOTS = os.environ.get("SHOTS", "/home/luka/.local/share/jogo-isolado/horizon/data/shots/abas")
TAG = os.environ.get("TAG", "depois")
LEFT, TOP = a.LEFT, a.TOP
ENERGY = (LEFT - 13, TOP + 150)
WARNING = (LEFT - 13, TOP + 122)
XP_BUTTON = (LEFT + 188, TOP + 103)
# O suporte da aba de xp (176, 90, 26 por 26) tem o botão de 18 por 18 em (179, 94): 3 pixels de borda à esquerda e 5 à direita não são elemento.
XP_HOLDER_LEFT = (LEFT + 177, TOP + 103)
XP_HOLDER_RIGHT = (LEFT + 199, TOP + 103)
FAR = (LEFT - 40, TOP + 50)


def shot(name):
    a.ctl("screenshot", f"{SHOTS}/{TAG}_{name}.png")


def open_screen():
    a.ctl("select", "6")
    for _ in range(4):
        a.ctl("interact", *map(str, SPOT))
        a.settle(0.7)
        if '"screen":"container"' in a.ctl("screen").replace(" ", ""):
            return
    raise SystemExit("o menu não abriu")


def place(machine, xp):
    block = MACHINES[machine][0]
    a.ctl("close")
    a.command("gamemode creative")
    a.command(f"tp @s {STAND}")
    a.settle(0.8)
    a.command("tick unfreeze")
    a.command(f"setblock {SPOT[0]} {SPOT[1]} {SPOT[2]} minecraft:air")
    a.settle(0.5)
    a.command("clear @s")
    a.ctl("give", block, "1")
    out = a.ctl("place", *map(str, SPOT), "--item", block)
    if "error" in json.loads(out):
        raise SystemExit(f"não pôs {machine}: {out[:200]}")
    a.settle(0.5)
    if xp:
        a.command('give @s mekanism:upgrade[mekanism:upgrade_type="mekanism:xp_containment"] 1')
        a.settle(0.5)
        a.ctl("select", "mekanism:upgrade")
        a.ctl("key", "shift", "down")
        a.ctl("interact", *map(str, SPOT))
        a.ctl("key", "shift", "up")
        a.ctl("wait", "30")
        a.command("clear @s")


def put_input(machine):
    """A entrada no slot de entrada pelo clique do jogador (a regra do slot vale): sem energia a máquina avisa."""
    _, item, slot = MACHINES[machine]
    if machine == "metallurgic_infuser":
        # Sem químico o infuser não rastreia a falta de energia (a receita não tem o que processar): 500 de bio no tanque, e a falta de energia aparece.
        a.command(f'data merge block {SPOT[0]} {SPOT[1]} {SPOT[2]} {{chemical_tanks:[{{container:0b,stored:{{type:"mekanism:bio",amount:500L}}}}]}}')
    a.give_hotbar(8, f"minecraft:{item}", 2)
    a.pick(8)
    a.ctl("click-slot", str(300 + slot))
    a.settle(1.5)


def hold_dirt():
    a.give_hotbar(8, "minecraft:dirt", 5)
    a.pick(8)
    return a.carried()


def click(point, button=None):
    a.ctl("click-at", str(point[0]), str(point[1]), *([button] if button else []))
    a.settle(0.5)
    return a.carried()


def state_case(machine, xp, warn, failures):
    label = f"{machine} xp={int(xp)} aviso={int(warn)}"
    place(machine, xp)
    open_screen()
    if warn:
        put_input(machine)
    hold_dirt()
    stays = [("aba de energia", ENERGY, True), ("aba de avisos", WARNING, warn), ("botão da aba de xp", XP_BUTTON, xp)]
    for name, point, kept in stays:
        if kept:
            expect_kept(f"{label}: clique na {name}", point, failures)
            # O botão direito não é um clique válido da aba (`isValidClickButton`), mas o elemento continua sob o mouse: `hasClickedOutside` é falso e o item fica.
            expect_kept(f"{label}: clique direito na {name}", point, failures, "right")
    # Os cliques que soltam: a aba que não existe no estado, a borda do suporte da aba de xp e a margem.
    drops = [(f"{label}: clique onde a aba de avisos não existe", WARNING)] if not warn else []
    drops += [(f"{label}: clique onde a aba de xp não existe", XP_BUTTON)] if not xp else []
    drops += [(f"{label}: clique na borda esquerda do suporte da aba de xp", XP_HOLDER_LEFT), (f"{label}: clique na borda direita do suporte da aba de xp", XP_HOLDER_RIGHT)] if xp else []
    drops += [(f"{label}: clique na margem, longe de tudo", FAR)]
    for name, point in drops:
        if a.carried() == ["", 0]:
            hold_dirt()
        point_label = name
        got = click(point)
        a.expect(point_label + ": solta", got, ["", 0], failures)
    a.ctl("close")


def expect_kept(label, point, failures, button=None):
    before = a.carried()
    if before == ["", 0]:
        before = hold_dirt()
    after = click(point, button)
    a.expect(label + ": o cursor continua", after, before, failures)
    if button is None and (label.endswith("energia") or label.endswith("avisos")):
        shot(label.replace(": clique na ", "_").replace(" ", "_").replace("=", ""))


def main():
    names = sys.argv[1:] or list(MACHINES)
    subprocess.run(["ssh", "desktop", "mkdir", "-p", SHOTS], check=True)  # a pasta é a do desktop, onde o container grava
    a.ctl("options-set", "guiScale", "3")
    failures = []
    for machine in names:
        print(f"-- {machine}", flush=True)
        for xp in (False, True):
            for warn in (False, True):
                state_case(machine, xp, warn, failures)
    a.ctl("close")
    print(f"{len(names) * 4} estados, {len(failures)} diferenças", flush=True)
    sys.exit(1 if failures else 0)


if __name__ == "__main__":
    main()
