#!/usr/bin/env python3
"""Repete no jogo, no container `horizon` (H_NAME=horizon), os casos do rastro heat_generator_armor_* que o jogo mostra: os cinco slots da
tela do Heat Generator (armadura e mão), o que cada clique deixa (colocar, recusar, pilha de 2, mão, trocar, número, F), o shift-clique de
ida e volta, o clique nas abas com item no cursor, a peça com maldição do vínculo em sobrevivência, e o que o servidor guarda (o `data get`
do inventário do jogador, que é o servidor, e não a tela do cliente).

Uso: H_NAME=horizon h2_armor.py [caso ...]   (sem argumentos roda todos; `prints` tira um print de cada estado em SHOTS)
Casos: layout, helmet, wrong_piece, stack_of_two, offhand, swap, number_key, offhand_key, shift_in, shift_out, tabs, frozen, binding, hud, events.
"""
import json, os, re, subprocess, sys, time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import h2_identity as h

# O mundo `mekanism` do desktop: terreno a 74, o gerador a 4 blocos ao norte do jogador.
SPOT = (-27, 74, -4)
STAND = "-26.5 74 -1.5 180 20"
BLOCK = "mekanismgenerators:heat_generator"
SHOTS = os.environ.get("SHOTS", "/home/luka/.local/share/jogo-isolado/horizon/data/shots/armadura")
# O canto da tela de 176 por 166 a guiScale 2 numa janela de 1280 por 720 (640 por 360 em pixels de GUI).
LEFT, TOP = 232, 97
HEAD, CHEST, LEGS, FEET, OFFHAND = 500, 501, 502, 503, 504


def ctl(*args):
    return h.ctl(*args)


def command(text):
    return h.command(text)


def j(text):
    try:
        return json.loads(text)
    except json.JSONDecodeError:
        return {"error": text[:200]}


def shot(name):
    if os.environ.get("PRINTS") == "1":
        ctl("screenshot", f"{SHOTS}/{name}.png")


def settle(seconds=0.5):
    time.sleep(seconds)


def prepare(game_mode="creative"):
    """O gerador posto, o inventário vazio, a tela aberta."""
    ctl("close")
    command(f"gamemode {game_mode}")
    command(f"tp @s {STAND}")
    settle(0.5)
    command("tick unfreeze")
    command("clear @s")
    command(f"data merge block {SPOT[0]} {SPOT[1]} {SPOT[2]} {{machinery_basis:{{v:1,tanks:[{{resource:\"minecraft:lava\",amount:1000L}}]}}}}")
    for _ in range(4):
        ctl("interact", *map(str, SPOT))
        settle(0.6)
        if '"screen":"container"' in ctl("screen").replace(" ", ""):
            return
    raise SystemExit("o menu não abriu")


def give_hotbar(slot, item, count=1, components=""):
    command(f"item replace entity @s hotbar.{slot} with {item}{components} {count}")
    settle(0.4)


def screen():
    return j(ctl("screen"))


def slot(number, scr=None):
    scr = scr or screen()
    for entry in scr.get("slots", []):
        if entry["slot"] == number:
            item = entry.get("item")
            return [item.split(":", 1)[1] if item else "", entry.get("count", 0) if item else 0]
    return ["", 0]


def carried(scr=None):
    scr = scr or screen()
    c = scr.get("carried")
    return [c["item"].split(":", 1)[1], c["count"]] if c else ["", 0]


def widget(number):
    ui = j(ctl("ui"))
    for w in ui["widgets"]:
        if w["id"] == f"slot-{number}":
            return w
    return None


def server_slot(part):
    """`data get entity @s equipment.<parte>`: o servidor, não a tela. Parte: head, chest, legs, feet ou offhand."""
    chat = h.command(f"data get entity @s equipment.{part}")
    return chat[0] if chat else ""


def expect(label, got, want, failures):
    ok = got == want
    print(("OK   " if ok else "DIFF ") + f"{label}: {got}" + ("" if ok else f" (esperado {want})"), flush=True)
    if not ok:
        failures.append(label)


def case_layout(failures):
    prepare()
    shot("layout")
    scr = screen()
    ids = [s["slot"] for s in scr["slots"] if s["slot"] >= 500]
    expect("slots 500 a 504 na tela", ids, [500, 501, 502, 503, 504], failures)
    positions = []
    for number in (HEAD, CHEST, LEGS, FEET, OFFHAND):
        w = widget(number)
        positions.append([w["bounds"][0] - LEFT, w["bounds"][1] - TOP] if w else None)
    # `armorSideBar(-20, 11, 0)`: x -20, y de 11 a 11 + 18 * 4.
    expect("posições (x, y relativos à tela)", positions, [[-20, 11], [-20, 29], [-20, 47], [-20, 65], [-20, 83]], failures)


def pick(hotbar_slot):
    ctl("click-slot", str(hotbar_slot))
    settle(0.3)


def case_helmet(failures):
    prepare()
    give_hotbar(8, "minecraft:iron_helmet")
    pick(8)
    ctl("click-slot", str(HEAD))
    settle(0.5)
    scr = screen()
    expect("capacete na cabeça: slot", slot(HEAD, scr), ["iron_helmet", 1], failures)
    expect("capacete na cabeça: cursor", carried(scr), ["", 0], failures)
    shot("capacete_na_cabeca")
    expect("capacete na cabeça: servidor", "iron_helmet" in server_slot("head"), True, failures)
    # Tirar de volta: o clique com o cursor vazio pega a peça.
    ctl("click-slot", str(HEAD))
    settle(0.5)
    scr = screen()
    expect("tirar: cursor", carried(scr), ["iron_helmet", 1], failures)
    expect("tirar: slot", slot(HEAD, scr), ["", 0], failures)


def case_wrong_piece(failures):
    prepare()
    give_hotbar(8, "minecraft:iron_helmet")
    pick(8)
    ctl("click-slot", str(FEET))
    settle(0.5)
    scr = screen()
    expect("capacete nas botas: slot", slot(FEET, scr), ["", 0], failures)
    expect("capacete nas botas: cursor", carried(scr), ["iron_helmet", 1], failures)
    shot("capacete_nas_botas_recusado")
    ctl("click-slot", str(CHEST))
    settle(0.4)
    expect("capacete no peitoral: slot", slot(CHEST), ["", 0], failures)
    # Terra não se veste em nenhum slot de armadura.
    ctl("click-slot", "8")
    settle(0.3)
    give_hotbar(7, "minecraft:dirt", 5)
    pick(7)
    ctl("click-slot", str(HEAD))
    settle(0.4)
    scr = screen()
    expect("terra na cabeça: slot", slot(HEAD, scr), ["", 0], failures)
    expect("terra na cabeça: cursor", carried(scr), ["dirt", 5], failures)


def case_stack_of_two(failures):
    prepare()
    give_hotbar(8, "minecraft:iron_helmet", 2, "[max_stack_size=4]")
    pick(8)
    ctl("click-slot", str(HEAD))
    settle(0.5)
    scr = screen()
    expect("pilha de 2 capacetes: slot", slot(HEAD, scr), ["iron_helmet", 1], failures)
    expect("pilha de 2 capacetes: cursor", carried(scr), ["iron_helmet", 1], failures)
    shot("pilha_de_dois_capacetes")


def case_offhand(failures):
    prepare()
    give_hotbar(8, "minecraft:dirt", 5)
    pick(8)
    ctl("click-slot", str(OFFHAND))
    settle(0.5)
    scr = screen()
    expect("terra na mão: slot", slot(OFFHAND, scr), ["dirt", 5], failures)
    expect("terra na mão: cursor", carried(scr), ["", 0], failures)
    shot("terra_na_mao")
    expect("terra na mão: servidor", "dirt" in server_slot("offhand"), True, failures)


def case_swap(failures):
    prepare()
    give_hotbar(8, "minecraft:iron_helmet")
    give_hotbar(7, "minecraft:leather_helmet")
    pick(8)
    ctl("click-slot", str(HEAD))
    settle(0.4)
    pick(7)
    ctl("click-slot", str(HEAD))
    settle(0.5)
    scr = screen()
    expect("trocar capacetes: slot", slot(HEAD, scr), ["leather_helmet", 1], failures)
    expect("trocar capacetes: cursor", carried(scr), ["iron_helmet", 1], failures)


def case_number_key(failures):
    prepare()
    give_hotbar(0, "minecraft:iron_helmet")
    w = widget(HEAD)
    cx, cy = int(w["bounds"][0] + 8), int(w["bounds"][1] + 8)
    ctl("scroll-at", str(cx), str(cy), "0")
    ctl("key", "1", "tap")
    settle(0.5)
    scr = screen()
    expect("tecla 1 sobre a cabeça: slot", slot(HEAD, scr), ["iron_helmet", 1], failures)
    expect("tecla 1 sobre a cabeça: barra", slot(0, scr), ["", 0], failures)
    shot("tecla_numerica")


def hover(number):
    w = widget(number)
    ctl("scroll-at", str(int(w["bounds"][0] + 8)), str(int(w["bounds"][1] + 8)), "0")


def case_offhand_key(failures):
    prepare()
    # `offhand_key_on_itself`: F sobre a própria mão não muda nada (`swap40` com a mão como origem).
    give_hotbar(0, "minecraft:dirt", 3)
    hover(OFFHAND)
    ctl("key", "f", "tap")
    settle(0.5)
    scr = screen()
    expect("F sobre a mão: nada muda", [slot(OFFHAND, scr), slot(0, scr)], [["", 0], ["dirt", 3]], failures)
    # `offhand_key_take`: F sobre a armadura leva a peça para a mão.
    command("item replace entity @s hotbar.0 with minecraft:air")
    command("item replace entity @s armor.head with minecraft:leather_helmet 1")
    settle(0.6)
    ctl("close")
    for _ in range(4):
        ctl("interact", *map(str, SPOT))
        settle(0.6)
        if '"screen":"container"' in ctl("screen").replace(" ", ""):
            break
    hover(HEAD)
    ctl("key", "f", "tap")
    settle(0.6)
    scr = screen()
    expect("F sobre a armadura: vai para a mão", [slot(HEAD, scr), slot(OFFHAND, scr)], [["", 0], ["leather_helmet", 1]], failures)
    shot("tecla_f")


def case_shift_in(failures):
    prepare()
    give_hotbar(8, "minecraft:iron_chestplate")
    command("item replace entity @s inventory.0 with minecraft:iron_boots 1")
    settle(0.4)
    ctl("click-slot", "8", "shift")
    settle(0.6)
    scr = screen()
    expect("shift do peitoral (barra): vai ao peitoral", slot(CHEST, scr), ["iron_chestplate", 1], failures)
    expect("shift do peitoral: sai da barra", slot(8, scr), ["", 0], failures)
    ctl("click-slot", "9", "shift")
    settle(0.6)
    scr = screen()
    expect("shift das botas (inventário): vai às botas", slot(FEET, scr), ["iron_boots", 1], failures)
    shot("shift_para_a_armadura")
    # Um item que não se veste, e a pilha de 2 capacetes: 1 na armadura e 1 no outro grupo, numa chamada só.
    give_hotbar(7, "minecraft:iron_helmet", 2, "[max_stack_size=4]")
    ctl("click-slot", "7", "shift")
    settle(0.6)
    scr = screen()
    expect("shift de 2 capacetes: 1 na cabeça", slot(HEAD, scr), ["iron_helmet", 1], failures)
    # `quickMoveStack` move a pilha toda: 1 para a armadura (só vazios) e o resto para o outro grupo (o inventário principal, o primeiro vazio).
    expect("shift de 2 capacetes: 1 no principal, a barra esvazia", [slot(7, scr), slot(9, scr)], [["", 0], ["iron_helmet", 1]], failures)


def case_shift_out(failures):
    prepare()
    command("item replace entity @s armor.chest with minecraft:iron_chestplate 1")
    settle(0.6)
    ctl("close")
    for _ in range(4):
        ctl("interact", *map(str, SPOT))
        settle(0.6)
        if '"screen":"container"' in ctl("screen").replace(" ", ""):
            break
    expect("o peitoral está no slot", slot(CHEST), ["iron_chestplate", 1], failures)
    ctl("click-slot", str(CHEST), "shift")
    settle(0.6)
    scr = screen()
    # `insertItem(playerSlots)`: a barra primeiro, o slot 0 é o primeiro vazio dela.
    expect("shift da armadura: vai à barra", [slot(CHEST, scr), slot(0, scr)], [["", 0], ["iron_chestplate", 1]], failures)
    shot("shift_da_armadura_para_a_barra")


def case_tabs(failures):
    prepare()
    give_hotbar(8, "minecraft:dirt", 5)
    pick(8)
    tabs = {"energia": (LEFT - 13, TOP + 137 + 13), "calor": (LEFT - 13, TOP + 109 + 13), "upgrades": (LEFT + 176 + 12, TOP + 10 + 8)}
    for name, (x, y) in tabs.items():
        before = carried()
        ctl("click-at", str(x), str(y))
        settle(0.5)
        expect(f"clique na aba de {name}: o cursor continua", carried(), before, failures)
        if name == "calor":
            # A aba de calor troca a unidade de temperatura do cliente (`config/mekanism/local.toml`): o clique direito a devolve.
            ctl("click-at", str(x), str(y), "right")
            settle(0.4)
            expect("clique direito na aba de calor: o cursor continua", carried(), before, failures)
    dropped = j(ctl("entities")).get("entities", [])
    expect("nenhum item solto no chão", [e for e in dropped if e.get("kind") == "item" and "dirt" in str(e.get("item")) and e.get("distance", 99) < 4], [], failures)
    shot("clique_nas_abas")
    # O suporte da coluna (sem elemento nenhum) e o fora da tela soltam, como no original.
    ctl("click-at", str(LEFT - 24), str(TOP + 50))
    settle(0.5)
    expect("clique no suporte, fora do slot: solta", carried(), ["", 0], failures)
    # Um slot da barra com o cursor cheio e a peça errada não é "fora": o cursor continua.
    give_hotbar(8, "minecraft:dirt", 5)
    pick(8)
    ctl("click-slot", str(HEAD))
    settle(0.4)
    expect("terra sobre o slot da cabeça: o cursor continua", carried(), ["dirt", 5], failures)


def case_frozen(failures):
    """Com o servidor parado: o que o clique mudou chega ao cliente (a injeção `bar_answer_click` o manda com a resposta do clique). Não distingue de
    um tick depois: o `item replace` com o servidor parado também chega (conferido), então a sincronização do menu do inventário não é só do tick."""
    prepare()
    give_hotbar(8, "minecraft:iron_helmet")
    pick(8)
    command("tick freeze")
    settle(0.5)
    ctl("click-slot", str(HEAD))
    settle(1.0)
    scr = screen()
    expect("servidor parado: o capacete está no slot", slot(HEAD, scr), ["iron_helmet", 1], failures)
    expect("servidor parado: o cursor esvaziou", carried(scr), ["", 0], failures)
    ctl("click-slot", str(HEAD))
    settle(1.0)
    scr = screen()
    expect("servidor parado: o capacete voltou ao cursor", [slot(HEAD, scr), carried(scr)], [["", 0], ["iron_helmet", 1]], failures)
    command("tick unfreeze")


def case_binding(failures):
    prepare("survival")
    cursed = '[enchantments={"minecraft:binding_curse":1}]'
    command(f"item replace entity @s armor.head with minecraft:leather_helmet{cursed} 1")
    settle(0.6)
    ctl("close")
    for _ in range(4):
        ctl("interact", *map(str, SPOT))
        settle(0.6)
        if '"screen":"container"' in ctl("screen").replace(" ", ""):
            break
    expect("peça com vínculo no slot", slot(HEAD)[0], "leather_helmet", failures)
    ctl("click-slot", str(HEAD))
    settle(0.5)
    scr = screen()
    expect("sobrevivência, clique: a peça fica", [slot(HEAD, scr)[0], carried(scr)], ["leather_helmet", ["", 0]], failures)
    ctl("click-slot", str(HEAD), "shift")
    settle(0.6)
    scr = screen()
    expect("sobrevivência, shift-clique: a peça fica", [slot(HEAD, scr)[0], slot(0, scr)], ["leather_helmet", ["", 0]], failures)
    shot("maldicao_do_vinculo")
    command("gamemode creative")


def case_hud(failures):
    prepare("survival")
    for part, item in (("head", "iron_helmet"), ("chest", "iron_chestplate")):
        command(f"item replace entity @s armor.{part} with minecraft:{item} 1")
    settle(0.6)
    ctl("close")
    settle(0.5)
    inv = j(ctl("inventory"))
    worn = {a["part"]: a["item"] for a in inv.get("armor", []) if a.get("item")}
    expect("equipamento no jogador", worn, {"head": "minecraft:iron_helmet", "chest": "minecraft:iron_chestplate"}, failures)
    shot("hud_de_armadura")
    command("gamemode creative")


SENSOR = (-30, 74, -2)


def sensor_phase():
    state = j(ctl("block", *map(str, SENSOR))).get("state", "")
    m = re.search(r"sculk_sensor_phase=(\w+)", state)
    return m.group(1) if m else "?"


def quiet():
    """Espera o sensor voltar a `inactive` (a vibração anterior acabou)."""
    for _ in range(60):
        if sensor_phase() == "inactive":
            return
        time.sleep(0.1)


def fired(action, seconds=1.2):
    """Faz `action` e diz se o sensor de sculk recebeu uma vibração (o jogo posta `minecraft:equip` ou `minecraft:unequip`)."""
    quiet()
    time.sleep(0.3)
    action()
    end = time.time() + seconds
    while time.time() < end:
        if sensor_phase() != "inactive":
            return True
        time.sleep(0.1)
    return False


def case_events(failures):
    """O evento de jogo de `onEquipItem` (`minecraft:equip` e `minecraft:unequip`), que um sensor de sculk ouve: o clique com o cursor o dispara
    (colocar, trocar e tirar da armadura), o shift-clique não (`quickMoveStack` move por `insert` e `remove`, sem `setByPlayer`), e a mão
    não emite evento (`Player.doesEmitEquipEvent`). O som de equipar (`item.armor.equip_*`) sai da mesma chamada, o `player_equipped`, mas
    o jogo sem tela não o expõe."""
    command(f"setblock {SENSOR[0]} {SENSOR[1]} {SENSOR[2]} minecraft:sculk_sensor")
    settle(1.0)
    prepare()
    give_hotbar(8, "minecraft:iron_helmet")
    pick(8)
    expect("clique: colocar o capacete emite equip", fired(lambda: ctl("click-slot", str(HEAD))), True, failures)
    expect("clique: tirar o capacete emite unequip", fired(lambda: ctl("click-slot", str(HEAD))), True, failures)
    ctl("click-slot", "8")
    settle(0.3)
    give_hotbar(8, "minecraft:iron_helmet")
    expect("shift-clique: o capacete do inventário para a armadura não emite", fired(lambda: ctl("click-slot", "8", "shift")), False, failures)
    expect("shift-clique: da armadura para a barra não emite", fired(lambda: ctl("click-slot", str(HEAD), "shift")), False, failures)
    give_hotbar(7, "minecraft:dirt", 5)
    pick(7)
    expect("clique: terra na mão não emite evento", fired(lambda: ctl("click-slot", str(OFFHAND))), False, failures)
    command(f"setblock {SENSOR[0]} {SENSOR[1]} {SENSOR[2]} minecraft:air")


CASES = {
    "layout": case_layout, "helmet": case_helmet, "wrong_piece": case_wrong_piece, "stack_of_two": case_stack_of_two,
    "offhand": case_offhand, "swap": case_swap, "number_key": case_number_key, "offhand_key": case_offhand_key,
    "shift_in": case_shift_in, "shift_out": case_shift_out, "tabs": case_tabs, "frozen": case_frozen, "binding": case_binding,
    "hud": case_hud, "events": case_events,
}


def main():
    names = sys.argv[1:] or list(CASES)
    failures = []
    for name in names:
        print(f"-- {name}", flush=True)
        CASES[name](failures)
    ctl("close")
    print(f"{len(names)} casos, {len(failures)} diferenças", flush=True)
    sys.exit(1 if failures else 0)


if __name__ == "__main__":
    main()
