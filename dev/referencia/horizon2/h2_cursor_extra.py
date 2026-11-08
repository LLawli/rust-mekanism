#!/usr/bin/env python3
"""Em jogo, no container `horizon` (H_NAME=horizon): o que `h2_cursor.py` não cobre do clique do cursor, contra as linhas que `SlotClickTraceTest.java` grava
em `slot_click_<máquina>.jsonl` e `slot_click_upgrade_<máquina>.jsonl`.

  throw      o Q (`THROW`, uma unidade) e o Ctrl+Q (a pilha inteira) sobre um slot da máquina, com o cursor vazio: o slot depois e quantos itens caíram no chão
  stacked    os baldes com `max_stack_size` 16 no componente (o cursor segura mais de um): clique esquerdo, direito, tecla numérica e arrasto
  upgrade    os dois slots virtuais da janela de upgrades (a janela aberta pela aba): cada tipo de upgrade na mão, com o slot vazio, com outro tipo e com o mesmo

O clique do meio (`CLONE`) e o clique duplo (`PICKUP_ALL`) o `horizon-ctl` não manda: `mouse` só age no mundo, e `click-at` só tem o esquerdo e o direito, sem o
duplo (medido: dois `click-at` seguidos num `script` pegam e põem de volta, nenhum `PICKUP_ALL`). Ficam só no rastro do Java.

Uso: h2_cursor_extra.py throw|stacked|upgrade MAQUINA [--todas]
"""
import json, os, sys, time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import h2_cursor as c
import h2_identity as h

TRACES = h.TRACES
TAB = (232 + 190, 97 + 19)  # a aba de upgrades, guiScale 3 numa janela de 1920 por 1080
STACKED = {"lava_bucket+16", "water_bucket+16"}
UPGRADE_TYPES = ["speed", "energy", "filter", "chemical", "muffling", "anchor", "stone_generator", "xp_containment"]


def comp_of(item):
    return "[minecraft:max_stack_size=16]" if item.endswith("+16") else ""


def base(item):
    return item.split("+")[0]


def item_spec(item):
    """Como o jogo escreve a pilha na linha de comando: o id e, nos baldes empilhados, o componente."""
    return f"{c.item_id(base(item))}{comp_of(item)}"


def set_slots(machine, row):
    """O estado do rastro (slot, entrada, tanque) pelo NBT, com o componente nos baldes empilhados."""
    x, y, z = c.SPOT[machine]
    kinds = c.KINDS[machine]
    items = {}
    slot, item, content = row["slot"], row["item"], row["content"]
    if row.get("input") and kinds[slot] != "input":
        items[kinds.index("input")] = (c.item_id(row["input"]), 1, "")
    if content == "other":
        items[slot] = ("minecraft:stick", 4, "")
    elif content == "same":
        comps = ',components:{"minecraft:max_stack_size":16}' if item.endswith("+16") else ""
        items[slot] = (c.item_id(base(item)), 4, comps)
    entries = ",".join(f'{{Slot:{i}b,id:"{name}",count:{count}{comps}}}' for i, (name, count, comps) in sorted(items.items()))
    tank = row["tank"]
    tanks = f'[{{container:0b,stored:{{type:"{c.CHEMICAL[tank]}",amount:500L}}}}]' if tank in c.CHEMICAL else "[]"
    patch = f"{{Items:[{entries}]"
    if machine == "metallurgic_infuser":
        patch += f",chemical_tanks:{tanks}"
    h.command(f"data merge block {x} {y} {z} {patch}}}")
    if machine == "metallurgic_infuser" and tank not in c.CHEMICAL:
        h.command(f"data remove block {x} {y} {z} chemical_tanks")
    if machine == "heat_generator":
        lava = "1000L" if tank == "lava" else "0L"
        resource = 'resource:"minecraft:lava",' if tank == "lava" else ""
        h.command(f'data merge block {x} {y} {z} {{machinery_basis:{{v:1,tanks:[{{{resource}amount:{lava}}}]}}}}')


def dropped():
    """Quantos itens soltos há perto do jogador (e some com eles, para o caso seguinte)."""
    total = 0
    for entity in json.loads(h.ctl("entities"))["entities"]:
        if entity["kind"] == "item":
            total += entity["item"]["count"]
    return total


def clear_drops():
    h.command("kill @e[type=item]")
    time.sleep(0.2)


def run_throw(machine, row):
    """[slot, itens no chão] depois do Q ou do Ctrl+Q sobre o slot."""
    h.ctl("close")
    clear_drops()
    set_slots(machine, row)
    h.command("clear @s")
    c.settle(3)
    c.open_menu(machine)
    time.sleep(0.4)
    target = 300 + row["slot"]
    cx, cy = c.center(c.widgets()[f"slot-{target}"])
    h.ctl("scroll-at", cx, cy, "0")
    if row["action"] == "throw_all":
        h.ctl("key", "ctrl", "down")
    h.ctl("key", "q", "tap")
    if row["action"] == "throw_all":
        h.ctl("key", "ctrl", "up")
    c.settle(2)
    time.sleep(0.4)
    scr = c.screen()
    return [c.slot_of(scr, target), dropped()]


def run_stacked(machine, row):
    """[slot, cursor, barra0, slot do arrasto] com um balde empilhado na mão."""
    item, action = row["item"], row["action"]
    h.ctl("close")
    set_slots(machine, row)
    h.command("clear @s")
    c.settle(3)
    c.open_menu(machine)
    spec = item_spec(item)
    if action == "number":
        h.command(f"item replace entity @s hotbar.0 with {spec} 3")
    else:
        h.command(f"item replace entity @s hotbar.8 with {spec} {6 if action == 'drag' else 3}")
    time.sleep(0.4)
    target = 300 + row["slot"]
    if action in ("left", "right"):
        h.ctl("click-slot", "8")
        h.ctl("click-slot", str(target)) if action == "left" else h.ctl("click-slot", str(target), "right")
    elif action == "number":
        cx, cy = c.center(c.widgets()[f"slot-{target}"])
        h.ctl("scroll-at", cx, cy, "0")
        h.ctl("key", "1", "tap")
    elif action == "drag":
        h.ctl("click-slot", "8")
        w = c.widgets()
        a, b = c.center(w[f"slot-{target}"]), c.center(w["slot-9"])
        h.ctl("drag-at", a[0], a[1], b[0], b[1])
    c.settle(2)
    time.sleep(0.3)
    scr = c.screen()
    return [c.slot_of(scr, target), c.carried(scr), c.slot_of(scr, 0), c.slot_of(scr, 9)]


def pick_rows(machine, kind, every):
    rows = [json.loads(line) for line in open(os.path.join(TRACES, f"slot_click_{machine}.jsonl"))][1:]
    if kind == "throw":
        rows = [r for r in rows if r["action"] in ("throw", "throw_all")]
        # Um por (tipo de slot, ação, conteúdo, item): o Q não olha o `mayPlace`, então poucos itens bastam.
        keep = {"stick", "redstone", "coal", "lava_bucket", "bucket", "dirt"}
        rows = [r for r in rows if r["item"] in keep]
    else:
        rows = [r for r in rows if r["item"] in STACKED]
    seen, chosen = set(), []
    for r in rows:
        key = (r["tank"], r["input"], r["kind"], r["action"], r["content"], r["item"])
        if machine == "metallurgic_infuser" and (r["tank"], r["input"]) != ("bio", "dirt") and not every:
            continue
        if key not in seen:
            seen.add(key)
            chosen.append(r)
    return chosen


def open_window(machine):
    c.open_menu(machine)
    h.ctl("click-at", str(TAB[0]), str(TAB[1]))
    time.sleep(0.7)
    scr = c.screen()
    # Os slots 500 em diante são a armadura do Heat Generator, não a janela.
    slots = sorted(s["slot"] for s in scr["slots"] if 300 <= s["slot"] < 500)
    return slots[-2], slots[-1]


def upgrade_spec(name, count):
    return f'mekanism:upgrade[mekanism:upgrade_type="mekanism:{name}"] {count}'


def upgrade_name(slot):
    """O tipo que o jogo mostra no slot, lido do `data get`... o `screen` só dá o item: o tipo vem do componente, que o `ui` não traz."""
    return slot


def run_upgrade(machine, first, last, row):
    """Devolve [slot de entrada ou saída, cursor, barra0, slot do arrasto] depois do clique, como as linhas do rastro."""
    kind, item, content, action = row["kind"], row["item"], row["content"], row["action"]
    target = first if kind == "upgrade_in" else last
    name = item.split(":", 1)[1] if item.startswith("upgrade:") else None
    spec = upgrade_spec(name, 3) if name else f"{c.item_id(item)} 3"
    h.command("clear @s")
    # O slot começa vazio (os dois): o conteúdo "other" e "same" é posto com um clique antes, como o jogador o poria.
    for slot in (first, last):
        scr = c.screen()
        if c.slot_of(scr, slot) and c.slot_of(scr, slot)[1]:
            h.ctl("click-slot", str(slot))
            h.ctl("click-slot", "9")
            h.command("clear @s")
    if content in ("other", "same") and kind == "upgrade_in":
        held = ("energy" if name == "speed" else "speed") if content == "other" else name
        h.command(f"item replace entity @s hotbar.7 with {upgrade_spec(held, 4)}")
        time.sleep(0.3)
        h.ctl("click-slot", "7")
        h.ctl("click-slot", str(first))
        time.sleep(0.4)
        h.command("clear @s")
    if action == "number":
        h.command(f"item replace entity @s hotbar.0 with {spec}")
    else:
        h.command(f"item replace entity @s hotbar.8 with {upgrade_spec(name, 6) if name and action == 'drag' else spec}")
    time.sleep(0.4)
    if action in ("left", "right"):
        h.ctl("click-slot", "8")
        h.ctl("click-slot", str(target)) if action == "left" else h.ctl("click-slot", str(target), "right")
    elif action == "number":
        cx, cy = c.center(c.widgets()[f"slot-{target}"])
        h.ctl("scroll-at", cx, cy, "0")
        h.ctl("key", "1", "tap")
    elif action == "drag":
        h.ctl("click-slot", "8")
        w = c.widgets()
        # A janela cobre o slot 9 do inventário: o segundo slot do arrasto é o último do inventário, que fica descoberto.
        a, b = c.center(w[f"slot-{target}"]), c.center(w["slot-35"])
        h.ctl("drag-at", a[0], a[1], b[0], b[1])
    time.sleep(0.5)
    scr = c.screen()
    return [c.slot_of(scr, target), c.carried(scr), c.slot_of(scr, 0), c.slot_of(scr, 35 if action == "drag" else 9)]


def upgrade_expected(row):
    def norm(stack):
        item, count = stack
        return ["upgrade" if item.startswith("upgrade:") else item, count]
    return [norm(row["slot_after"]), norm(row["carried"]), norm(row["hotbar0"]), norm(row["drain"])]


def main():
    mode, machine = sys.argv[1], sys.argv[2]
    every = "--todas" in sys.argv
    h.command("tick unfreeze")
    c.place(machine)
    bad = 0
    started = time.time()
    if mode == "upgrade":
        rows = [json.loads(line) for line in open(os.path.join(TRACES, f"slot_click_upgrade_{machine}.jsonl"))][1:]
        # Um por (slot, conteúdo, ação, aceita) e dois tipos aceitos: a regra olha o tipo, o clique não.
        seen, chosen = {}, []
        for r in rows:
            # O slot de saída só tem conteúdo se o jogo o pôs por dentro (a remoção da janela), e o roteiro não o faz: só os casos de saída vazia.
            if r["kind"] == "upgrade_out" and r["content"] != "empty":
                continue
            key = (r["kind"], r["content"], r["action"], r["may_place"])
            if every or seen.get(key, 0) < (3 if r["may_place"] else 1):
                seen[key] = seen.get(key, 0) + 1
                chosen.append(r)
        h.ctl("close")
        # Servidor parado: a instalação de 20 ticks não anda entre o clique que prepara o slot e o clique que se mede (o rastro lê no mesmo tick).
        h.command("tick freeze")
        first, last = open_window(machine)
        for r in chosen:
            got = run_upgrade(machine, first, last, r)
            want = upgrade_expected(r)
            norm = [[x[0] if x[0] != "upgrade" else "upgrade", x[1]] if x else x for x in got]
            label = f"{machine} {r['kind']} {r['action']} {r['content']} {r['item']} aceita={r['may_place']}"
            if norm == want:
                print(f"OK   {label}", flush=True)
            else:
                bad += 1
                print(f"DIFF {label}: esperado {want}, obtido {got}", flush=True)
        h.command("tick unfreeze")
        print(f"{machine} upgrade: {len(chosen)} casos, {bad} diferenças, {time.time() - started:.0f} s")
        sys.exit(1 if bad else 0)
    rows = pick_rows(machine, mode, every)
    h.command("tick unfreeze")
    if machine in ("heat_generator", "metallurgic_infuser"):
        h.command("tick freeze")
        c.FROZEN = True
    for r in rows:
        label = f"{machine} tanque={r['tank']} {r['kind']} {r['action']} {r['content']} {r['item']}"
        if mode == "throw":
            got = run_throw(machine, r)
            want = [r["slot_after"], r["dropped"]]
            ok = got == want
        else:
            got = run_stacked(machine, r)
            want = c.expected(r)
            ok = got == want
        if ok:
            print(f"OK   {label}", flush=True)
        else:
            bad += 1
            print(f"DIFF {label}: esperado {want}, obtido {got}", flush=True)
    h.command("tick unfreeze")
    print(f"{machine} {mode}: {len(rows)} casos, {bad} diferenças, {time.time() - started:.0f} s")
    sys.exit(1 if bad else 0)


if __name__ == "__main__":
    main()
