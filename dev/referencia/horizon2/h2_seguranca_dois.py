#!/usr/bin/env python3
"""A segurança com dois jogadores em jogo (frente `seguranca`, decisões 250 a 253): o anfitrião põe uma Enrichment Chamber (vira o dono) e o convidado de LAN, sem acesso, usa o
Configurator (agachado), o cartão de configuração e a chave inglesa, abre a tela e quebra. Roda no container que tem a trava (`H_NAME=horizon` ou `horizon2`), com os dois clientes de
`abrir_dois.sh` e a LAN aberta. O mundo é o plano `spike`, com a máquina em y -60.

Cada linha impressa começa com OK ou DIFF. Etapas (uma por chamada, ou `all`):
  publico    num bloco sem dono o convidado, agachado, troca o lado com o Configurator (de pé não troca)
  privado    o dono passa a máquina a PRIVATE pela aba; o convidado não abre a tela, não troca o lado, não copia o cartão, não desmonta com a chave e não quebra, e recebe a mensagem
  dono       o dono, no mesmo bloco PRIVATE, faz tudo isso
Uso: H_NAME=horizon h2_seguranca_dois.py [etapa...]"""
import json, os, re, sys, time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import h2_dois as d

GUEST = d.GUEST_NAME
POS = (74, -60, 64)
OTHER = (78, -60, 64)
CHAMBER = "mekanism:enrichment_chamber"
CONFIGURATOR = "mekanism:configurator"
CARD = "mekanism:configuration_card"
MODE = "mekanism:configurator_mode"
TAB = (188, 47)  # o centro do botão da aba de segurança, em pixels de GUI relativos à tela
SCALE = 3
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


def who(role):
    return d.host if role == "dono" else d.guest


def name(role):
    return "@s" if role == "dono" else GUEST


def screen_open(role):
    return js(who(role)("screen")).get("screen")


def data(pos, path):
    return " ".join(hcmd(f"data get block {pos[0]} {pos[1]} {pos[2]} {path}"))


def sides(pos, key="config3"):
    found = re.search(r"\[I; ([-\d, ]+)\]", data(pos, f"component_config.{key}"))
    return [int(v) for v in found.group(1).split(",")] if found else None


def mode_of(pos):
    found = re.search(r'"(\w+)"', data(pos, "component_security.security_mode"))
    return found.group(1) if found else "public"


def block_is(pos, block):
    return any("passed" in line.lower() for line in hcmd(f"execute if block {pos[0]} {pos[1]} {pos[2]} {block}"))


def give(role, item):
    who(role)("close")
    hcmd(f"clear {name(role)}")
    hcmd(f"give {name(role)} {item}")
    time.sleep(0.5)
    who(role)("select", item.split("[")[0])


def click(role, pos, face, shift):
    ctl = who(role)
    ctl("close")
    if shift:
        ctl("key", "shift", "down")
    out = ctl("interact", *map(str, pos), face)
    time.sleep(0.4)
    if shift:
        ctl("key", "shift", "up")
    time.sleep(0.6)
    return out


def chat_of(role):
    return who(role)("messages")


def setup():
    hcmd("gamerule advance_time false")
    hcmd("gamerule advance_weather false")
    hcmd("weather clear")
    hcmd("time set noon")
    hcmd("gamemode creative @s")
    hcmd(f"gamemode survival {GUEST}")
    for pos in (POS, OTHER):
        hcmd(f"setblock {pos[0]} {pos[1]} {pos[2]} minecraft:air")
    time.sleep(0.5)
    hcmd(f"tp {GUEST} 74.5 -60 61.5 0 20")
    hcmd("tp @s 75.5 -60 61.5 0 20")
    time.sleep(1.0)
    # O dono põe a máquina (o jogador que põe vira o dono, `TileComponentSecurity.setOwnerUUID`).
    hcmd("clear @s")
    hcmd(f"give @s {CHAMBER}")
    time.sleep(0.5)
    out = js(d.host("place", *map(str, POS), "--item", CHAMBER))
    time.sleep(1.0)
    return "error" not in out


def tab_click():
    """O dono abre a tela e clica na aba (o cursor em pixels de GUI da janela: o canto da tela mais o ponto dentro dela)."""
    d.host("options-set", "guiScale", str(SCALE))
    d.host("options-set", "chatVisibility", "0")
    d.host("close")
    d.host("interact", *map(str, POS), "north")
    time.sleep(1.5)
    gui_l, gui_a = -(-1920 // SCALE), -(-1080 // SCALE)
    left, top = (gui_l - 176) // 2, (gui_a - 166) // 2
    d.host("click-at", str(left + TAB[0]), str(top + TAB[1]))
    time.sleep(0.6)
    d.host("close")


def publico():
    ok = report(setup(), "o dono pôs a máquina")
    ok &= report("owner" in data(POS, "component_security"), f"o dono ficou no tile: {data(POS, 'component_security')[:90]}")
    give("convidado", f'{CONFIGURATOR}[{MODE}="configurate_items"]')
    before = sides(POS)
    click("convidado", POS, "north", shift=False)
    ok &= report(sides(POS) == before, "o convidado de pé não troca o lado")
    click("convidado", POS, "north", shift=True)
    after = sides(POS)
    ok &= report(after != before, f"o convidado agachado troca o lado: {before} -> {after}")
    return ok


def privado():
    ok = report(setup(), "o dono pôs a máquina")
    tab_click()
    ok &= report(mode_of(POS) == "private", f"o dono passou o modo a private pela aba: {mode_of(POS)}")
    # abrir a tela
    who("convidado")("close")
    click("convidado", POS, "north", shift=False)
    ok &= report(screen_open("convidado") in NO_SCREEN, f"o convidado não abre a tela: {screen_open('convidado')}")
    # o Configurator agachado
    give("convidado", f'{CONFIGURATOR}[{MODE}="configurate_items"]')
    before = sides(POS)
    click("convidado", POS, "north", shift=True)
    ok &= report(sides(POS) == before, "o convidado agachado não troca o lado")
    ok &= report("access" in chat_of("convidado").lower(), "a mensagem de sem acesso chegou ao convidado")
    # a chave inglesa desmonta
    give("convidado", f'{CONFIGURATOR}[{MODE}="wrench"]')
    click("convidado", POS, "north", shift=True)
    ok &= report(block_is(POS, CHAMBER), "o convidado não desmonta a máquina com a chave inglesa")
    # o cartão copia
    give("convidado", CARD)
    click("convidado", POS, "north", shift=True)
    held = js(d.guest("state")).get("held") or {}
    ok &= report("data_name" not in json.dumps(held), "o convidado não copia os dados com o cartão")
    # quebrar
    out = d.guest("break", *map(str, POS))
    time.sleep(1.5)
    ok &= report(block_is(POS, CHAMBER), f"o convidado não quebra o bloco ({out[:80]})")
    return ok


def dono():
    ok = True
    give("dono", f'{CONFIGURATOR}[{MODE}="configurate_items"]')
    before = sides(POS)
    click("dono", POS, "north", shift=True)
    ok &= report(sides(POS) != before, f"o dono agachado troca o lado: {before} -> {sides(POS)}")
    give("dono", CARD)
    click("dono", POS, "north", shift=True)
    held = js(d.host("state")).get("held") or {}
    ok &= report("data_name" in json.dumps(held), "o dono copia os dados com o cartão (o controle da recusa do convidado)")
    give("dono", f'{CONFIGURATOR}[{MODE}="wrench"]')
    click("dono", POS, "north", shift=True)
    ok &= report(not block_is(POS, CHAMBER), "o dono desmonta a máquina com a chave inglesa (o controle da recusa do convidado)")
    return ok


def main():
    steps = sys.argv[1:] or ["all"]
    if "all" in steps:
        steps = ["publico", "privado", "dono"]
    ok = True
    for step in steps:
        ok &= globals()[step]()
    print("TUDO IGUAL" if ok else "HÁ DIFERENÇAS")
    sys.exit(0 if ok else 1)


if __name__ == "__main__":
    main()
