#!/usr/bin/env python3
"""Em jogo, no container `horizon` (H_NAME=horizon): o upgrade na mão, com shift, sobre o bloco (`ItemUpgrade.useOn`, `ItemUpgrade.java:28-48`) instala desde o
primeiro uso, sem a tela da máquina ter aberto nunca: `addUpgrades` instala até o máximo do tipo (`TileComponentUpgrade.addUpgrades`) e o item perde o que entrou
(`stack.consume(added, player)`, só em sobrevivência). Cada máquina é posta por um jogador, sem abrir tela, e o jogador passa a sobrevivente antes do uso.

Uso: h2_upgrade_use.py [MAQUINA ...]   (padrão: as quatro que falhavam e a Enrichment Chamber; abra o jogo e não abra nenhuma tela antes)
"""
import json, os, re, sys, time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import h2_cursor as c
import h2_identity as h

# O que o original faz com 20 de velocidade (máximo 8) e 3 de silêncio (máximo 1): quantos entram e quantos sobram na mão.
PLAN = {
    "crusher": ("speed", 20, 8, 12),
    "energized_smelter": ("speed", 20, 8, 12),
    "metallurgic_infuser": ("speed", 20, 8, 12),
    "heat_generator": ("muffling", 3, 1, 2),
    "enrichment_chamber": ("speed", 20, 8, 12),
}


def hand_count():
    held = json.loads(h.ctl("state"))["held"]
    return held["count"] if held and held["item"] == "mekanism:upgrade" else 0


def main():
    machines = sys.argv[1:] or list(PLAN)
    ok = True
    for machine in machines:
        kind, count, installed_want, left_want = PLAN[machine]
        h.command("tick unfreeze")
        h.ctl("close")
        h.command("gamemode creative")
        c.place(machine)
        x, y, z = c.SPOT[machine]
        h.command("clear @s")
        h.command("gamemode survival")
        h.command(f'give @s mekanism:upgrade[mekanism:upgrade_type="mekanism:{kind}"] {count}')
        time.sleep(0.5)
        h.ctl("select", "mekanism:upgrade")
        h.ctl("key", "shift", "down")
        out = h.ctl("interact", str(x), str(y), str(z))
        h.ctl("key", "shift", "up")
        h.ctl("wait", "10")
        text = h.command(f"data get block {x} {y} {z} component_upgrade")[0]
        found = re.search(rf'"mekanism:{kind}": (\d+)', text)
        installed = int(found.group(1)) if found else 0
        left = hand_count()
        good = (installed, left) == (installed_want, left_want) and "error" not in out
        ok &= good
        print(("OK   " if good else "DIFF ") + f"{machine}: instalados {installed} (original {installed_want}), na mão {left} (original {left_want})")
    h.command("gamemode creative")
    print("TUDO IGUAL" if ok else "HÁ DIFERENÇAS")
    sys.exit(0 if ok else 1)


if __name__ == "__main__":
    main()
