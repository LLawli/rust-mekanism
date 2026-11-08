#!/usr/bin/env python3
"""Em jogo (`H_NAME=horizon`): as receitas do data pack `mektest` com o ingrediente `neoforge:components` na Enrichment Chamber.

O data pack (em `dev/referencia/testes/dados/mektest`) tem: `named_stick` (graveto com `custom_name` "Foo" dá diamante), `cost_flint` (sílex com `repair_cost` 5
dá barra de ouro) e `strict_coal` (carvão sem nada a mais, `strict`, dá esmeralda), mais receitas que o mod recusa e o log tem de explicar. Cada caso põe o item
na entrada da máquina, com a energia posta por `data merge` (o Heat Generator no deserto de pedra dá pouca), e espera uma operação. Uso: h2_componentes.py
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


def main():
    h.command("tick unfreeze")
    x, y, z = BASE
    for pos in [(x, y, z), (x, y, z + 1)]:
        h.command(f"setblock {pos[0]} {pos[1]} {pos[2]} minecraft:air")
    time.sleep(0.5)
    h.ctl("setblock", str(x), str(y), str(z), "mekanism:enrichment_chamber")
    h.ctl("setblock", str(x), str(y), str(z + 1), "mekanismgenerators:heat_generator")
    time.sleep(3)
    cases = [
        ("graveto Foo", stack("stick", '"minecraft:custom_name":"Foo"'), "diamond"),
        ("graveto sem nome", stack("stick"), None),
        ("graveto Bar", stack("stick", '"minecraft:custom_name":"Bar"'), None),
        ("sílex repair_cost 5", stack("flint", '"minecraft:repair_cost":5'), "gold_ingot"),
        ("sílex repair_cost 6", stack("flint", '"minecraft:repair_cost":6'), None),
        ("sílex sem nada", stack("flint"), None),
        ("carvão padrão (strict)", stack("coal"), "emerald"),
        # O `strict` pede o carvão sem nada a mais: o com nome cai na receita simples de carvão (`enriched_carbon`).
        ("carvão com nome (strict)", stack("coal", '"minecraft:custom_name":"Foo"'), "enriched_carbon"),
        ("osso (compound recusado)", stack("bone"), None),
    ]
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
        print(("OK     " if good else "FALHOU ") + name, "| esperado:", product or "nada", "| máquina:", items)
        h.command(f"data merge block {x} {y} {z} {{Items:[]}}")
    print("TUDO OK" if ok else "HÁ FALHAS")
    sys.exit(0 if ok else 1)


if __name__ == "__main__":
    main()
