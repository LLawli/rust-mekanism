#!/usr/bin/env python3
"""Em jogo (`H_NAME=horizon`): as receitas do data pack `mektest` com o ingrediente `neoforge:components` na Enrichment Chamber.

O data pack (em `dev/referencia/testes/dados/mektest`) tem: `named_stick` (graveto com `custom_name` "Foo" dá diamante), `cost_flint` (sílex com `repair_cost` 5
dá barra de ouro) e `strict_coal` (carvão sem nada a mais, `strict`, dá esmeralda), mais receitas que o mod recusa e o log tem de explicar. Cada caso põe o item
na entrada da máquina, com a energia posta por `data merge` (o Heat Generator no deserto de pedra dá pouca), e espera uma operação. Uso: h2_componentes.py [chamber|smelter]

O Smelter lê o cozimento do jogo: `smelt_named_stick`, `smelt_compound` (um `neoforge:compound` com um filho de componentes e um osso) e `smelt_tag` (`items` com `#minecraft:coals`).
"""
import re, sys, time
import h2_identity as h

BASE = (90, -60, 70)
WAIT = 14


def put(pos, item):
    h.command(f"data merge block {pos[0]} {pos[1]} {pos[2]} {{Items:[{item}]}}")


def stack(item, components="", count=1):
    comps = f",components:{{{components}}}" if components else ""
    return f'{{Slot:0b,id:"minecraft:{item}",count:{count}{comps}}}'


def run_cases(machine, base, cases):
    x, y, z = base
    h.command(f"setblock {x} {y} {z} minecraft:air")
    time.sleep(0.5)
    h.ctl("setblock", str(x), str(y), str(z), machine)
    time.sleep(2)
    ok = True
    for name, item, product in cases:
        put((x, y, z), item)
        h.command(f"data merge block {x} {y} {z} {{energy:100000}}")
        time.sleep(WAIT)
        items = h.read_items((x, y, z))
        made = [it for it in items if it[1] == product] if product else []
        kept = [it for it in items if it[0] == 0]
        good = bool(made) if product else (not [it for it in items if it[0] == 1] and bool(kept))
        ok &= good
        print(("OK     " if good else "FALHOU ") + f"{machine.split(':')[1]}: {name}", "| esperado:", product or "nada", "| máquina:", items)
        h.command(f"data merge block {x} {y} {z} {{Items:[]}}")
    return ok


def main():
    h.command("tick unfreeze")
    chamber = [
        ("graveto Foo", stack("stick", '"minecraft:custom_name":"Foo"'), "diamond"),
        ("graveto sem nome", stack("stick"), None),
        ("graveto Bar", stack("stick", '"minecraft:custom_name":"Bar"'), None),
        ("sílex repair_cost 5", stack("flint", '"minecraft:repair_cost":5'), "gold_ingot"),
        ("sílex repair_cost 6", stack("flint", '"minecraft:repair_cost":6'), None),
        ("sílex sem nada", stack("flint"), None),
        ("carvão padrão (strict)", stack("coal"), "emerald"),
        # O `strict` pede o carvão sem nada a mais: o com nome cai na receita simples de carvão (`enriched_carbon`).
        ("carvão com nome (strict)", stack("coal", '"minecraft:custom_name":"Foo"'), "enriched_carbon"),
        # A receita `compound_bone` (um `neoforge:compound` de um osso) agora é lida: antes o mod a recusava com o aviso.
        ("osso (compound)", stack("bone"), "diamond"),
    ]
    # O Smelter lê o cozimento do jogo (`smelting`), que o Horizon só entrega se o `neoforge:components` passa pela injeção `components-ingredient`.
    smelter = [
        ("graveto Foo", stack("stick", '"minecraft:custom_name":"Foo"'), "gold_nugget"),
        ("graveto sem nome", stack("stick"), None),
        ("graveto Bar", stack("stick", '"minecraft:custom_name":"Bar"'), None),
        ("compound: sílex repair_cost 5", stack("flint", '"minecraft:repair_cost":5'), "iron_nugget"),
        ("compound: sílex sem nada", stack("flint"), None),
        ("compound: osso", stack("bone"), "iron_nugget"),
        ("tag: carvão repair_cost 7", stack("coal", '"minecraft:repair_cost":7'), "copper_ingot"),
        ("tag: carvão repair_cost 8", stack("coal", '"minecraft:repair_cost":8'), None),
    ]
    ok = run_cases("mekanism:enrichment_chamber", BASE, chamber) if "smelter" not in sys.argv[1:] else True
    ok &= run_cases("mekanism:energized_smelter", (BASE[0], BASE[1], BASE[2] + 6), smelter) if "chamber" not in sys.argv[1:] else True
    print("TUDO OK" if ok else "HÁ FALHAS")
    sys.exit(0 if ok else 1)


if __name__ == "__main__":
    main()
