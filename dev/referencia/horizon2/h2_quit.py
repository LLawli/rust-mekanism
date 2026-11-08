#!/usr/bin/env python3
"""Em jogo, no container `horizon` (H_NAME=horizon): `Save and Quit` com a janela de upgrades aberta, contra `upgrade_quit_<máquina>.jsonl`
(`UpgradeQuitTraceTest.java`) e `upgrade_quit.rs`. No original o tile é gravado com o menu aberto e relido do disco: os upgrades que estão no slot da
janela, os instalados, a energia, a entrada e o progresso voltam como estavam, e a instalação recomeça dos 20 ticks (`upgradeTicks` e
`canCheckUpgrades` não são gravados).

Três máquinas com upgrades no slot da janela, instalando (servidor parado, `tick step` conta os ticks): o Metallurgic Infuser parado por falta de
químico (progresso e energia exatos), o Heat Generator com carvão queimando, e a Enrichment Chamber andando, com a janela dela aberta na hora do
`quit`. Os tipos e quantidades passam do máximo de cada tipo (energia 8, velocidade 8, silêncio 1), então sobra no slot depois da instalação, que
termina logo depois de reabrir: o que sobra (e o que foi instalado) mostra que nada se perdeu nem duplicou.

Uso, em duas etapas com o jogo fechado e aberto de novo entre elas:
  h2_quit.py antes    monta, anota o estado (QUIT_STATE) e faz `quit` com a janela da Enrichment Chamber aberta
  h2_quit.py depois   lê o mundo reaberto e compara (a instalação termina nos primeiros 20 ticks)
"""
import json, os, re, sys, time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import h2_identity as h
import h2_cursor as c

STATE = os.environ.get("QUIT_STATE", "/tmp/quit_state.json")
# Onde o jogo (dentro do container) grava os prints: a pasta de dados é a mesma no host e no container.
SHOTS = os.environ.get("SHOTS_DIR", "")
# A aba de upgrades de uma tela de 176 por 166 em guiScale 3, numa janela de 1920 por 1080 (canto em (232, 97), aba em (190, 19)).
TAB = (232 + 190, 97 + 19)
BLOCKS = {**h.BLOCK, "heat_generator": "mekanismgenerators:heat_generator"}
# O que cada máquina recebe no slot da janela e o que sobra depois de instalar (o máximo do tipo vem dos dados de upgrade).
PLAN = {
    "metallurgic_infuser": dict(upgrade="speed", count=30, installed=8, left=22),
    "heat_generator": dict(upgrade="muffling", count=3, installed=1, left=2),
    "enrichment_chamber": dict(upgrade="energy", count=40, installed=8, left=32),
}
ORDER = ["metallurgic_infuser", "heat_generator", "enrichment_chamber"]


def pos(machine):
    return " ".join(map(str, c.SPOT[machine]))


def get(machine, path=""):
    chat = h.command(f"data get block {pos(machine)} {path}".strip())[0]
    return chat.split("block data: ", 1)[-1] if "block data: " in chat else chat


def number(text, key):
    found = re.search(rf"\b{key}: (-?\d+)", text)
    return int(found.group(1)) if found else None


def upgrades(machine):
    """O que o NBT guarda do componente: os instalados (`upgrades`) e os dois slots (`input`, `output`)."""
    text = get(machine, "component_upgrade")
    installed = {m.group(1): int(m.group(2)) for m in re.finditer(r'"mekanism:(\w+)": (\d+)', re.sub(r"input:.*", "", text, flags=re.S))}
    return text, installed


def slot_items(machine):
    """Os itens de `Items` por slot, como (id, quantidade): os slots virtuais da janela vêm depois dos da máquina."""
    text = get(machine, "Items")
    items = {}
    for body in re.findall(r"\{[^{}]*(?:\{[^{}]*\}[^{}]*)*\}", text):
        slot = re.search(r"Slot: (\d+)b", body)
        ident = re.search(r'id: "([^"]+)"', body)
        count = re.search(r"count: (\d+)", body)
        if slot and ident and count:
            items[int(slot.group(1))] = (ident.group(1), int(count.group(1)), body)
    return items


def snapshot(machine):
    text, installed = upgrades(machine)
    tank = re.search(r"amount: (\d+)L", get(machine, "chemical_tanks")) if machine == "metallurgic_infuser" else None
    return {
        "tank": int(tank.group(1)) if tank else None,
        "energy": int(re.search(r"(-?\d+)", get(machine, "energy")).group(1)),
        "progress": int(re.search(r"(-?\d+)", get(machine, "progress")).group(1)) if machine != "heat_generator" else None,
        "installed": installed,
        "component": text,
        "items": {slot: [i, n] for slot, (i, n, _) in slot_items(machine).items()},
    }


def place_all():
    h.command("tick unfreeze")
    h.ctl("close")
    for machine in ORDER:
        c.place(machine)
    time.sleep(0.5)
    # A Enrichment Chamber andando, com o que a energia deixa; o Infuser e o gerador montados abaixo.
    x, y, z = c.SPOT["enrichment_chamber"]
    h.command(f'data merge block {x} {y} {z} {{Items:[{{Slot:0b,id:"minecraft:clay",count:8}}],energy:1000}}')
    x, y, z = c.SPOT["metallurgic_infuser"]
    h.command(
        f'data merge block {x} {y} {z} {{Items:[{{Slot:1b,id:"minecraft:dirt",count:6}}],energy:1000,'
        f'chemical_tanks:[{{container:0b,stored:{{type:"mekanism:bio",amount:500L}}}}]}}'
    )
    x, y, z = c.SPOT["heat_generator"]
    # `c.place` enche o tanque de lava do gerador para ele não queimar o combustível dos testes de clique: aqui ele tem de queimar.
    h.command(f'data merge block {x} {y} {z} {{Items:[{{Slot:0b,id:"minecraft:coal",count:8}}],machinery_basis:{{v:1,tanks:[{{amount:0L}}]}}}}')


def open_window(machine):
    # O jogador vai até a máquina (o servidor parado não anda, mas o teleporte vale): o alcance do clique direito é curto.
    x, y, z = c.SPOT[machine]
    h.ctl("close")
    h.ctl("tp", str(x), str(y), str(z - 3))
    time.sleep(0.6)
    c.open_menu(machine)
    h.ctl("click-at", str(TAB[0]), str(TAB[1]))
    time.sleep(0.7)
    scr = c.screen()
    # Os slots 500 em diante são a armadura do Heat Generator, não a janela.
    slots = sorted(s["slot"] for s in scr["slots"] if 300 <= s["slot"] < 500)
    return slots[-2], slots[-1]


def put_upgrades(machine):
    plan = PLAN[machine]
    first, last = open_window(machine)
    h.command("clear @s")
    h.command(f'item replace entity @s hotbar.8 with mekanism:upgrade[mekanism:upgrade_type="mekanism:{plan["upgrade"]}"] {plan["count"]}')
    time.sleep(0.4)
    h.ctl("click-slot", "8")
    h.ctl("click-slot", str(first))
    time.sleep(0.4)
    scr = c.screen()
    got = c.slot_of(scr, first)
    print(f"  {machine}: slot da janela {first} depois do clique: {got}, cursor {c.carried(scr)}")
    return first, last


def antes():
    place_all()
    h.command("tick freeze")
    time.sleep(0.3)
    # O Infuser: 40 ticks de operação, e então a energia cai a 3 (abaixo do uso por tick): ele para no meio, com progresso, energia e tanque exatos.
    h.command("tick step 40")
    time.sleep(0.35 + 0.05 * 40)
    x, y, z = c.SPOT["metallurgic_infuser"]
    h.command(f"data merge block {x} {y} {z} {{energy:3}}")
    time.sleep(0.3)
    for machine in ORDER:
        put_upgrades(machine)
        if machine != ORDER[-1]:
            h.ctl("close")
    # A janela da Enrichment Chamber continua aberta. 8 ticks de instalação em todas (o `upgradeTicks` de cada uma chega a 8).
    h.command("tick step 8")
    time.sleep(0.35 + 0.05 * 8)
    state = {machine: snapshot(machine) for machine in ORDER}
    screen = c.screen()
    state["window_open"] = screen.get("screen")
    state["window_slots"] = [(s["slot"], s.get("item"), s.get("count")) for s in screen["slots"] if s["slot"] >= 300]
    for machine in ORDER:
        print(machine, json.dumps({k: v for k, v in state[machine].items() if k != "component"}))
    print("tela aberta:", state["window_open"], state["window_slots"])
    json.dump(state, open(STATE, "w"), indent=1)
    if SHOTS:
        print(h.ctl("screenshot", f"{SHOTS}/quit_antes.png"))
    print("quit com a janela aberta")
    print(h.ctl("quit"))


def depois():
    before = json.load(open(STATE))
    h.command("tick freeze")
    # O mundo reaberto não nasce parado: o servidor andou do carregamento até aqui, e a instalação (20 ticks) já terminou.
    ok = True
    for machine in ORDER:
        plan = PLAN[machine]
        was, now = before[machine], snapshot(machine)
        key = f"mekanism:{plan['upgrade']}"
        left = [n for (slot, (i, n)) in ((s, v) for s, v in now["items"].items()) if "upgrade" in i]
        installed = now["installed"].get(plan["upgrade"], 0)
        slot_left = sum(left)
        good = installed == plan["installed"] and slot_left == plan["left"]
        extra = ""
        if machine == "metallurgic_infuser":
            same = (now["progress"], now["energy"], now["tank"]) == (was["progress"], was["energy"], was["tank"])
            good &= same and was["progress"] > 0
            extra = f" progresso {was['progress']} -> {now['progress']}, energia {was['energy']} -> {now['energy']}, tanque {was['tank']} -> {now['tank']}"
        elif machine == "enrichment_chamber":
            good &= now["progress"] is not None and now["progress"] >= was["progress"]
            extra = f" progresso {was['progress']} -> {now['progress']}, energia {was['energy']} -> {now['energy']}"
        else:
            extra = f" energia {was['energy']} -> {now['energy']}"
        ok &= good
        print(("OK   " if good else "DIFF ") + f"{machine}: instalados {installed} (esperado {plan['installed']}), sobra no slot {slot_left} (esperado {plan['left']}){extra}")
        print("     itens:", now["items"])
    if SHOTS:
        # A janela da Enrichment Chamber de novo, com o que sobrou no slot de entrada (o jogo reaberto).
        open_window("enrichment_chamber")
        time.sleep(0.5)
        print(h.ctl("screenshot", f"{SHOTS}/quit_depois.png"))
        h.ctl("close")
    h.command("tick unfreeze")
    print("TUDO IGUAL" if ok else "HÁ DIFERENÇAS")
    sys.exit(0 if ok else 1)


if __name__ == "__main__":
    {"antes": antes, "depois": depois}[sys.argv[1]]()
