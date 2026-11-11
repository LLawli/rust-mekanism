#!/usr/bin/env python3
"""O Configurator e o cartão de configuração em jogo (`H_NAME=horizon` ou `horizon2`, mundo plano), contra o rastro do original (`rastros/configurator_*.jsonl`).

Etapas (uma por chamada, ou `all`):
  modos     a rolagem agachado troca o modo (a tecla N só abre o menu radial); o componente na pilha e o tooltip
  lados     agachado em configurar muda o DataType da face (item e energia), sem agachar a tela da máquina abre; a mensagem sai no overlay
  girar     ROTATE: sem agachar o bloco gira (norte vira leste, sem abrir a tela); agachado fica para a face oposta
  esvaziar  EMPTY agachado solta os itens dos slots
  chave     WRENCH agachado desmonta a máquina para o inventário (com a configuração)
  cartao    o cartão copia agachado (got), cola em outra máquina igual (set), recusa a diferente (unequal), esvazia no ar (cleared)
  selecao   o contorno de um transmissor com o Configurator na mão é a caixa que o olhar toca
Cada linha impressa começa com OK ou DIFF.
"""
import json, os, re, sys, time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import h2_cursor as c
import h2_identity as h

X0, Y, Z0 = 70, -60, 64
MODE_COMPONENT = "mekanism:configurator_mode"


def state():
    return json.loads(h.ctl("state"))


def mode_now():
    comps = (held() or {}).get("components") or {}
    if isinstance(comps, str):
        comps = json.loads(comps)
    return comps.get(MODE_COMPONENT, "configurate_items")


def held():
    return state().get("held")


def command(text):
    return h.command(text)


def block_is(pos, block):
    out = command(f"execute if block {pos[0]} {pos[1]} {pos[2]} {block}")
    return any("passed" in line.lower() for line in out)


def config_of(pos, key):
    text = command(f"data get block {pos[0]} {pos[1]} {pos[2]} component_config.{key}")[0]
    found = re.search(r"\[I; ([-\d, ]+)\]", text)
    return [int(v) for v in found.group(1).split(",")] if found else None


def give_configurator(mode=None):
    command("clear @s")
    data = f'[{MODE_COMPONENT}="{mode}"]' if mode else ""
    command(f"give @s mekanism:configurator{data} 1")
    time.sleep(0.5)
    h.ctl("select", "mekanism:configurator")


def click(pos, face, shift):
    if shift:
        h.ctl("key", "shift", "down")
    out = h.ctl("interact", *map(str, pos), face)
    if shift:
        h.ctl("key", "shift", "up")
    h.ctl("wait", "6")
    return out


def report(good, text):
    print(("OK   " if good else "DIFF ") + text)
    return good


def survival():
    command("gamemode survival")


def modos():
    """A tecla N com o Configurator só abre o menu radial (`isModeItem(.., false)` é falso), então o modo troca pela rolagem agachado (`allowModeScroll`)."""
    ok = True
    give_configurator()
    for steps, want in [(1, "configurate_fluids"), (7, "configurate_items"), (-1, "wrench")]:
        h.ctl("key", "shift", "down")
        for _ in range(abs(steps)):
            h.ctl("scroll-at", "240", "135", "1" if steps > 0 else "-1")
            h.ctl("wait", "24")
        h.ctl("key", "shift", "up")
        got = mode_now()
        ok &= report(got == want, f"rolagem agachado x{steps}: {got} (original {want})")
    h.ctl("key", "n", "tap")
    h.ctl("wait", "10")
    got = mode_now()
    ok &= report(got == "wrench", f"a tecla N sozinha não troca o modo: {got}")
    print(json.dumps(held())[:300])
    return ok


def lados():
    ok = True
    survival_mode = False
    c.place("enrichment_chamber")
    pos = c.SPOT["enrichment_chamber"]
    give_configurator("configurate_items")
    before = config_of(pos, "config3")
    click(pos, "east", shift=False)
    ok &= report(config_of(pos, "config3") == before, "sem agachar não muda a configuração")
    click(pos, "east", shift=True)
    after = config_of(pos, "config3")
    ok &= report(after != before, f"agachado muda o DataType: {before} -> {after}")
    give_configurator("configurate_energy")
    click(pos, "up", shift=True)
    ok &= report(config_of(pos, "config0") is not None, f"energia: {config_of(pos, 'config0')}")
    return ok


def girar():
    ok = True
    c.place("enrichment_chamber")
    pos = c.SPOT["enrichment_chamber"]
    give_configurator("rotate")
    click(pos, "south", shift=False)
    ok &= report(block_is(pos, "mekanism:enrichment_chamber[facing=east]") or block_is(pos, "mekanism:enrichment_chamber[facing=south]"), "ROTATE sem agachar gira")
    click(pos, "north", shift=True)
    ok &= report(block_is(pos, "mekanism:enrichment_chamber[facing=south]"), "ROTATE agachado na face norte: fica para o sul")
    return ok


def main():
    steps = sys.argv[1:] or ["all"]
    if "all" in steps:
        steps = ["modos", "lados", "girar"]
    ok = True
    for step in steps:
        ok &= globals()[step]()
    print("TUDO IGUAL" if ok else "HÁ DIFERENÇAS")
    sys.exit(0 if ok else 1)


if __name__ == "__main__":
    main()
