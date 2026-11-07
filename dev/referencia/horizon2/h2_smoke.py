#!/usr/bin/env python3
"""Teste rápido no horizon2: as quatro máquinas processam com a energia de um Heat Generator encostado, o som toca e o gerador gera."""
import json, re, sys, time
import h2_identity as h

MACHINES = [
    ("enrichment_chamber", 0, "clay", 1, "clay_ball"),
    ("crusher", 0, "cobblestone", 1, "gravel"),
    ("energized_smelter", 0, "sand", 1, "glass"),
    ("metallurgic_infuser", 1, "dirt", 2, "podzol"),
]
BASE = (90, -60, 70)


def main():
    h.command("tick unfreeze")
    placed = []
    for i, (name, inp, item, out, product) in enumerate(MACHINES):
        x, y, z = BASE[0] + i * 4, BASE[1], BASE[2]
        for pos in [(x, y, z), (x, y, z + 1), (x + 1, y, z)]:
            h.command(f"setblock {pos[0]} {pos[1]} {pos[2]} minecraft:air")
        time.sleep(0.3)
        h.ctl("setblock", str(x), str(y), str(z), h.BLOCK[name])
        h.ctl("setblock", str(x), str(y), str(z + 1), "mekanismgenerators:heat_generator")
        placed.append((x, y, z))
    time.sleep(3)
    for i, (name, inp, item, out, product) in enumerate(MACHINES):
        x, y, z = placed[i]
        items = [f"{{Slot:{inp}b,id:\"minecraft:{item}\",count:16}}"]
        if name == "metallurgic_infuser":
            items.append("{Slot:0b,id:\"mekanism:bio_fuel\",count:16}")
        h.command(f"data merge block {x} {y} {z} {{Items:[{','.join(items)}]}}")
        h.command(f"data merge block {x} {y} {z+1} {{Items:[{{Slot:0b,id:\"minecraft:coal\",count:64}}]}}")
    time.sleep(25)
    ok = True
    for i, (name, inp, item, out, product) in enumerate(MACHINES):
        x, y, z = placed[i]
        items = h.read_items((x, y, z))
        made = [it for it in items if it[1] == product and it[0] == out]
        gen = h.command(f"data get block {x} {y} {z+1} machinery_basis")[0][-160:]
        energy = re.search(r"energy: (\d+)", h.command(f"data get block {x} {y} {z+1} machinery_basis")[0]) 
        print(("OK   " if made else "FALHOU ") + name, "saída:", made, "| gerador:", gen)
        ok &= bool(made)
    st = json.loads(h.ctl("status"))
    print("sons ouvidos:", st.get("heard"))
    print("TUDO OK" if ok else "HÁ FALHAS")


if __name__ == "__main__":
    main()
