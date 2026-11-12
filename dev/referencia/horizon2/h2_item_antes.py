#!/usr/bin/env python3
"""O item age antes do bloco (decisão 47, H72) com dois clientes: o anfitrião e o convidado de LAN usam o Configurator e o cartão sobre uma máquina e sobre o cubo de
energia, contra os rastros `configurator_card.jsonl` e `configurator_click_*.jsonl`. Roda no container que tem a trava (`H_NAME=horizon` ou `horizon2`), com os dois
clientes de `abrir_dois.sh` e a LAN aberta.

Cada linha impressa começa com OK ou DIFF. Etapas (uma por chamada, ou `all`):
  cartao_convidado   o convidado copia agachado no cubo (o cartão guarda os dados e NÃO é esvaziado logo depois: era o `use` duplicado), cola numa máquina igual, sem abrir tela
  cartao_anfitriao   o mesmo, com o anfitrião
  girar              o Configurator em ROTATE gira a máquina e o cubo sem abrir tela (convidado e anfitrião); em ITEMS a tela abre
  agachado           agachado, de mãos vazias, a máquina e o cubo não abrem a tela (`openGui` passa)
  tela               de pé e de mãos vazias a tela da máquina e a do cubo abrem
Uso: H_NAME=horizon2 h2_item_antes.py [etapa...]"""
import json, os, sys, time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import h2_dois as d

GUEST = d.GUEST_NAME
CHAMBER_A, CHAMBER_B, CUBE = (74, -59, 64), (76, -59, 64), (78, -59, 64)
CARD = "mekanism:configuration_card"
CONFIGURATOR = "mekanism:configurator"
MODE = "mekanism:configurator_mode"
DATA = "mekanism:configuration_data"
NO_SCREEN = (None, "", "none", "playing")


def report(good, text):
    print(("OK   " if good else "DIFF ") + text, flush=True)
    return good


def hcmd(text):
    return json.loads(d.host("command", text))["chat"]


def js(raw):
    try:
        return json.loads(raw)
    except json.JSONDecodeError:
        return {}


def who_ctl(role):
    return d.host if role == "anfitriao" else d.guest


def name_of(role):
    return "@s" if role == "anfitriao" else GUEST


def screen_open(role):
    return js(who_ctl(role)("screen")).get("screen")


def held(role):
    return (js(who_ctl(role)("state")).get("held")) or {}


def card_has_data(role):
    """O NBT da pilha na mão (`data get entity ... SelectedItem`): o cartão com dados traz `data_name` no componente."""
    name = "Player90366" if role == "anfitriao" else GUEST
    text = " ".join(hcmd(f"data get entity {name} SelectedItem"))
    return "data_name" in text


def block_is(pos, block):
    return any("passed" in line.lower() for line in hcmd(f"execute if block {pos[0]} {pos[1]} {pos[2]} {block}"))


def machines():
    return ((CHAMBER_A, "mekanism:enrichment_chamber"), (CHAMBER_B, "mekanism:enrichment_chamber"), (CUBE, "mekanism:basic_energy_cube"))


def setup():
    hcmd("gamerule advance_time false")
    hcmd("gamerule advance_weather false")
    hcmd("weather clear")
    hcmd("time set noon")
    hcmd("gamemode creative @s")
    hcmd(f"gamemode creative {GUEST}")
    for pos, _ in machines():
        hcmd(f"setblock {pos[0]} {pos[1]} {pos[2]} minecraft:air")
    time.sleep(0.5)
    for pos, block in machines():
        hcmd(f"setblock {pos[0]} {pos[1]} {pos[2]} {block}[facing=north]")
    time.sleep(1.0)
    hcmd(f"tp {GUEST} 76.5 -59 60.5 0 20")
    hcmd("tp @s 76.5 -59 61.5 0 20")
    time.sleep(1.0)


def clear_hand(role):
    who_ctl(role)("close")
    hcmd(f"clear {name_of(role)}")


def give(role, item):
    clear_hand(role)
    hcmd(f"give {name_of(role)} {item}")
    time.sleep(0.5)
    who_ctl(role)("select", item.split("[")[0])


def click(role, pos, face, shift):
    ctl = who_ctl(role)
    ctl("close")
    if shift:
        ctl("key", "shift", "down")
    out = ctl("interact", *map(str, pos), face)
    time.sleep(0.4)
    if shift:
        ctl("key", "shift", "up")
    time.sleep(0.4)
    return out


def card_steps(role):
    ok = True
    ctl = who_ctl(role)
    give(role, CARD)
    # Copiar agachado no cubo: SUCCESS_SERVER, o cartão guarda os dados e segue com eles (o `use` do mesmo clique não o esvazia).
    click(role, CUBE, "north", shift=True)
    time.sleep(1.0)
    ok &= report(card_has_data(role), f"{role}: agachado no cubo o cartão guarda os dados (e segue com eles um segundo depois)")
    ok &= report(screen_open(role) in NO_SCREEN, f"{role}: a tela do cubo não abre agachado com o cartão ({screen_open(role)})")
    # Colar de pé no cubo: o cartão passa a vez da tela (`openGui` TRY_WITH_EMPTY_HAND), a tela não abre, e o cartão segue com os dados.
    click(role, CUBE, "north", shift=False)
    ok &= report(screen_open(role) in NO_SCREEN, f"{role}: de pé o cartão cola no cubo e a tela não abre ({screen_open(role)})")
    ok &= report(card_has_data(role), f"{role}: depois de colar o cartão segue com os dados")
    # Máquina: copiar agachado, colar de pé em outra igual.
    give(role, CARD)
    click(role, CHAMBER_A, "north", shift=True)
    time.sleep(1.0)
    ok &= report(card_has_data(role), f"{role}: agachado na máquina o cartão guarda os dados")
    click(role, CHAMBER_B, "north", shift=False)
    ok &= report(screen_open(role) in NO_SCREEN, f"{role}: de pé o cartão cola na outra máquina e a tela não abre ({screen_open(role)})")
    # No ar agachado: esvazia.
    # Olhando para o céu: o `use` no ar (sobre uma máquina o cartão copiaria).
    hcmd(f"tp {name_of(role)} 76.5 -59 6{1 if role == 'anfitriao' else 0}.5 0 -45")
    time.sleep(0.5)
    ctl("key", "shift", "down")
    ctl("use-item")
    time.sleep(0.5)
    ctl("key", "shift", "up")
    time.sleep(0.5)
    ok &= report(not card_has_data(role), f"{role}: no ar, agachado, o cartão esvazia")
    hcmd(f"tp {name_of(role)} 76.5 -59 6{1 if role == 'anfitriao' else 0}.5 0 20")
    return ok


def girar(role):
    ok = True
    for pos, label in ((CHAMBER_A, "máquina"), (CUBE, "cubo")):
        block = "mekanism:enrichment_chamber" if pos == CHAMBER_A else "mekanism:basic_energy_cube"
        hcmd(f"setblock {pos[0]} {pos[1]} {pos[2]} {block}[facing=north]")
        time.sleep(0.5)
        give(role, f'{CONFIGURATOR}[{MODE}="rotate"]')
        click(role, pos, "north", shift=False)
        ok &= report(not block_is(pos, f"{block}[facing=north]"), f"{role}: ROTATE de pé gira o {label} (o norte deixa de ser o rumo)")
        ok &= report(screen_open(role) in NO_SCREEN, f"{role}: ROTATE de pé não abre a tela do {label} ({screen_open(role)})")
        give(role, f'{CONFIGURATOR}[{MODE}="configurate_items"]')
        click(role, pos, "north", shift=False)
        ok &= report(screen_open(role) not in NO_SCREEN, f"{role}: em ITEMS de pé a tela do {label} abre ({screen_open(role)})")
        who_ctl(role)("close")
    return ok


def agachado(role):
    ok = True
    clear_hand(role)
    for pos, label in ((CHAMBER_A, "máquina"), (CUBE, "cubo")):
        click(role, pos, "north", shift=True)
        ok &= report(screen_open(role) in NO_SCREEN, f"{role}: agachado, de mãos vazias, a tela do {label} não abre ({screen_open(role)})")
    return ok


def tela(role):
    ok = True
    clear_hand(role)
    for pos, label in ((CHAMBER_A, "máquina"), (CUBE, "cubo")):
        click(role, pos, "north", shift=False)
        ok &= report(screen_open(role) not in NO_SCREEN, f"{role}: de pé, de mãos vazias, a tela do {label} abre ({screen_open(role)})")
        who_ctl(role)("close")
    return ok


STEPS = {
    "cartao_convidado": lambda: card_steps("convidado"),
    "cartao_anfitriao": lambda: card_steps("anfitriao"),
    "girar": lambda: girar("convidado") & girar("anfitriao"),
    "agachado": lambda: agachado("convidado") & agachado("anfitriao"),
    "tela": lambda: tela("convidado") & tela("anfitriao"),
}


def main():
    steps = sys.argv[1:] or ["all"]
    if "all" in steps:
        steps = list(STEPS)
    setup()
    ok = True
    for step in steps:
        ok &= bool(STEPS[step]())
    print("TUDO IGUAL" if ok else "HÁ DIFERENÇAS")
    sys.exit(0 if ok else 1)


if __name__ == "__main__":
    main()
