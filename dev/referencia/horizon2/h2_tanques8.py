#!/usr/bin/env python3
"""Teste em jogo da 8.ª rodada dos tanques, no port (`H_NAME=horizon`). Uso: t8.py PARTE..."""
import os, re, sys, time, json
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
os.environ.setdefault("H_NAME", "horizon")
import h2_identity as h
X, Y, Z = 55, 141, 80
def c(text):
    out = h.command(text)
    return " | ".join(out)[:900]
def tank(data): 
    m = re.search(r"tanks: \[\{([^}]*)\}", data); 
    return m.group(1) if m else ""
def item_near(x, y, z):
    return c(f"data get entity @e[type=item,x={x},y={y},z={z},distance=..3,limit=1,sort=nearest] Item")
def setup():
    print(c("gamerule advance_time false"), c("gamerule advance_weather false"), c("time set noon"), c("weather clear"))
    print(c(f"fill {X-6} 140 {Z-6} {X+20} 140 {Z+6} minecraft:smooth_stone"))
    print(c(f"fill {X-6} 141 {Z-6} {X+20} 146 {Z+6} minecraft:air"))
    print(c(f"tp @s {X+5} 142 {Z-5} 0 20"))
    c("kill @e[type=item]")
def destruir():
    for i, (block, nbt) in enumerate([
        ("mekanism:basic_fluid_tank", '{machinery_basis:{v:1,tanks:[{resource:"minecraft:lava",amount:5000L}]}}'),
        ("mekanism:basic_chemical_tank", None),
        ("mekanism:basic_energy_cube", None),
    ]):
        x = X + 3 * i
        c("kill @e[type=item]")
        print(block, c(f"setblock {x} 141 {Z} {block}"))
        time.sleep(0.6)
        if nbt:
            print(" merge", c(f"data merge block {x} 141 {Z} {nbt}"))
        print(" slot", c(f"container {x} 141 {Z} minecraft:bucket 1"))
        time.sleep(0.6)
        print(" antes", c(f"data get block {x} 141 {Z}")[:500])
        print(" destroy", c(f"setblock {x} 141 {Z} minecraft:air destroy"))
        time.sleep(0.8)
        print(" itens", c(f"execute if entity @e[type=item,x={x},y=141,z={Z},distance=..3]"))
        print(" item", item_near(x, 141, Z))
def cano(ativo="1b", destino=None):
    c("kill @e[type=item]")
    x = X + 12
    c(f"fill {x-1} 141 {Z-1} {x+4} 143 {Z+1} minecraft:air")
    print(c(f"fill {x} 141 {Z} {x+2} 141 {Z} mekanism:basic_mechanical_pipe"))
    print(c(f"setblock {x} 142 {Z} mekanism:basic_fluid_tank"))
    if destino:
        print(c(f"setblock {x+3} 141 {Z} {destino}"))
    time.sleep(1.5)
    c("tick freeze")
    print(c(f'data merge block {x} 142 {Z} {{active_state:{ativo},machinery_basis:{{v:1,tanks:[{{resource:"minecraft:water",amount:20000L}}]}}}}'))
    linhas = []
    for t in range(10):
        c("tick step 1"); time.sleep(0.4)
        d = c(f"data get block {x} 142 {Z}")
        p = c(f"data get block {x} 141 {Z}")
        q = c(f"data get block {x+3} 141 {Z}") if destino else ""
        m = re.search(r"amount: (\d+)L", tank(d)); n = re.search(r"stored: \{[^}]*\}|fluid[a-z_]*: \{[^}]*\}", p)
        linhas.append((t, m.group(1) if m else "0", n.group(0) if n else re.sub(r".*block data: ", "", p)[:200], tank(q)))
        print(*linhas[-1])
    c("tick unfreeze")
    return linhas
def cano_inativo():
    cano("0b")
def cano_destino():
    cano("1b", "mekanism:advanced_fluid_tank")
for part in sys.argv[1:]:
    if part in ("setup", "destruir", "cano", "cano_inativo", "cano_destino"):
        globals()[part]()

def slots():
    c("kill @e[type=item]")
    x = X
    print(c(f"setblock {x} 141 {Z} mekanism:basic_fluid_tank"))
    time.sleep(0.6)
    c(f'data merge block {x} 141 {Z} {{machinery_basis:{{v:1,tanks:[{{resource:"minecraft:lava",amount:5000L}}]}},edit_mode:"fill"}}')
    print(" container", h.ctl("container", str(x), "141", str(Z), "minecraft:bucket", "1")[:200])
    time.sleep(0.8)
    print(" antes", re.sub(r".*block data: ", "", c(f"data get block {x} 141 {Z}"))[:420])
    print(" destroy", c(f"setblock {x} 141 {Z} minecraft:air destroy"))
    time.sleep(0.8)
    print(" itens no chão", c(f"execute if entity @e[type=item,x={x},y=141,z={Z},distance=..4]"))
    print(" item", item_near(x, 141, Z))
def explosao():
    c("kill @e[type=item]")
    x = X + 6
    print(c(f"setblock {x} 141 {Z} mekanism:basic_fluid_tank"))
    time.sleep(0.6)
    c(f'data merge block {x} 141 {Z} {{machinery_basis:{{v:1,tanks:[{{resource:"minecraft:lava",amount:7000L}}]}}}}')
    print(" container", h.ctl("container", str(x), "141", str(Z), "minecraft:bucket", "1")[:120])
    time.sleep(0.5)
    print(c(f"summon minecraft:tnt {x}.5 142.2 {Z}.5 {{fuse:5}}"))
    time.sleep(2.0)
    print(" bloco", c(f"execute if block {x} 141 {Z} mekanism:basic_fluid_tank"))
    print(" itens no chão", c(f"execute if entity @e[type=item,x={x},y=141,z={Z},distance=..8]"))
    print(" item", item_near(x, 141, Z))
    print(c(f"fill {X-6} 140 {Z-6} {X+20} 140 {Z+6} minecraft:smooth_stone"))
def pistao():
    c("kill @e[type=item]")
    x = X + 3
    print(c(f"setblock {x} 141 {Z+3} mekanism:basic_fluid_tank"))
    print(c(f"setblock {x} 141 {Z+4} minecraft:piston[facing=north]"))
    print(c(f"setblock {x} 141 {Z+5} minecraft:redstone_block"))
    time.sleep(1.5)
    print(" pistão", c(f"execute if block {x} 141 {Z+4} minecraft:piston[extended=true]"), "| tanque no lugar", c(f"execute if block {x} 141 {Z+3} mekanism:basic_fluid_tank"))
    print(" itens", c(f"execute if entity @e[type=item,x={x},y=141,z={Z+3},distance=..4]"))
    c(f"fill {x} 141 {Z+2} {x} 141 {Z+5} minecraft:air")
for part in sys.argv[1:]:
    if part in ("slots", "explosao", "pistao"):
        globals()[part]()
