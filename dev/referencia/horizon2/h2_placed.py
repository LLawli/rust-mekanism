#!/usr/bin/env python3
"""Mede no horizon2 a capacidade de energia das quatro máquinas conforme o jeito de chegarem ao mundo, contra
`placed_<máquina>.jsonl` (PlacedTraceTest.java), e uma operação inteira de uma máquina posta por um jogador, contra `placed_run_*`.

A capacidade não está no NBT: o que se mede é a energia que a máquina guarda quando um Heat Generator encostado a enche, parada e sem
entrada. Uma máquina posta por um jogador, sem upgrade, enche até 4 ticks de receita (1.000, 1.600 no Crusher); a do `setblock`, e a lida do
disco sem upgrade, até 4 ticks de consumo (20, 32 no Crusher).

Etapas, uma por chamada:
  build    põe, por jogador, cada máquina em z=70 e, por `setblock`, cada uma em z=76, cada uma com um Heat Generator encostado e carvão
  upgrade  instala 1 de velocidade na Enrichment Chamber do jogador (a de 1.341 do Java)
  read     espera o gerador encher as máquinas e imprime a energia de cada uma
  drain    zera a energia e salva o mundo (depois feche o jogo, abra de novo e rode `read`)
  gap      a máquina do jogador fica fora da distância de simulação (carregada, sem andar) por 600 ticks e volta: mantém a capacidade (Java: o tile é o mesmo)
  unload   o chunk descarrega de verdade e volta lido do disco: a máquina sem upgrade volta a 20, como o Java
  run      uma operação inteira de uma máquina do jogador, com a energia posta de uma vez e o servidor parado
"""
import json, os, re, sys, time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import h2_identity as h

MACHINES = ["enrichment_chamber", "crusher", "energized_smelter", "metallurgic_infuser"]
X = {name: 70 + 4 * i for i, name in enumerate(MACHINES)}
PLAYER_Z, SETBLOCK_Z = 70, 76
CAPACITY = {  # (posta por jogador, setblock ou lida do disco sem upgrade)
    "enrichment_chamber": (1000, 20),
    "crusher": (1600, 32),
    "energized_smelter": (1000, 20),
    "metallurgic_infuser": (1000, 20),
}
GENERATOR = "mekanismgenerators:heat_generator"
FILL_TICKS = 300


def stored(pos):
    chat = h.command(f"data get block {pos[0]} {pos[1]} {pos[2]} energy")[0]
    found = re.search(r"data: (-?\d+)", chat) or re.search(r"(-?\d+)$", chat)
    return int(found.group(1)) if found else None


def build():
    h.command("tick unfreeze")
    for name in MACHINES:
        x = X[name]
        for z in (PLAYER_Z, PLAYER_Z + 1, SETBLOCK_Z, SETBLOCK_Z + 1):
            h.command(f"setblock {x} -60 {z} minecraft:air")
    time.sleep(1)
    for name in MACHINES:
        x = X[name]
        h.ctl("close")
        h.ctl("tp", str(x), "-60", str(PLAYER_Z - 3))
        h.ctl("wait", "5")
        h.command("clear @s")
        h.ctl("give", h.BLOCK[name], "1")
        out = json.loads(h.ctl("place", str(x), "-60", str(PLAYER_Z), "--item", h.BLOCK[name]))
        assert "error" not in out, out
        h.ctl("setblock", str(x), "-60", str(SETBLOCK_Z), h.BLOCK[name])
        for z in (PLAYER_Z + 1, SETBLOCK_Z + 1):
            h.ctl("setblock", str(x), "-60", str(z), GENERATOR)
    h.ctl("wait", "10")
    for name in MACHINES:
        for z in (PLAYER_Z + 1, SETBLOCK_Z + 1):
            h.command(f'data merge block {X[name]} -60 {z} {{Items:[{{Slot:0b,id:"minecraft:coal",count:64}}]}}')
    print("montado")


def upgrade():
    name = "enrichment_chamber"
    x = X[name]
    h.ctl("close")
    h.ctl("tp", str(x), "-60", str(PLAYER_Z - 3))
    h.ctl("wait", "5")
    h.command("clear @s")
    h.command('give @s mekanism:upgrade[mekanism:upgrade_type="mekanism:speed"] 1')
    time.sleep(0.5)
    h.ctl("key", "shift", "down")
    print(h.ctl("interact", str(x), "-60", str(PLAYER_Z))[:160])
    h.ctl("key", "shift", "up")
    h.ctl("wait", "30")
    print(h.command(f"data get block {x} -60 {PLAYER_Z} component_upgrade")[0][-120:])


def read(expect_player=None):
    h.ctl("wait", str(FILL_TICKS))
    ok = True
    for name in MACHINES:
        x = X[name]
        player, template = stored((x, -60, PLAYER_Z)), stored((x, -60, SETBLOCK_Z))
        want_player, want_template = CAPACITY[name]
        if expect_player is not None:
            want_player = expect_player.get(name, want_player)
        flag = "OK  " if (player, template) == (want_player, want_template) else "DIFF"
        ok &= flag == "OK  "
        print(f"{flag} {name}: posta por jogador {player} (esperado {want_player}), setblock {template} (esperado {want_template})")
    return ok


def drain():
    for name in MACHINES:
        for z in (PLAYER_Z, SETBLOCK_Z):
            h.command(f"data merge block {X[name]} -60 {z} {{energy:0}}")
    h.ctl("wait", "5")
    print(h.ctl("save")[:100])


def away(x, z, ticks, decoy=True):
    """Leva o jogador a (x, z); com `decoy`, põe ali uma máquina que anda (a varredura dos objetos vivos corre quando outra máquina anda)."""
    h.ctl("close")
    h.ctl("tp", str(x), "-60", str(z - 3))
    h.ctl("wait", "10")
    if decoy:
        h.command(f"setblock {x} -61 {z} minecraft:grass_block")
        h.command(f"setblock {x} -60 {z} minecraft:air")
        h.command("clear @s")
        h.ctl("give", h.BLOCK["enrichment_chamber"], "1")
        h.ctl("place", str(x), "-60", str(z), "--item", h.BLOCK["enrichment_chamber"])
    h.ctl("wait", str(ticks))


def back():
    x = X["enrichment_chamber"]
    h.ctl("tp", str(x), "-60", str(PLAYER_Z - 3))
    h.ctl("wait", "10")


def zero():
    for name in MACHINES:
        for z in (PLAYER_Z, SETBLOCK_Z):
            h.command(f"data merge block {X[name]} -60 {z} {{energy:0}}")


def generators(present):
    """Tira ou põe de novo os Heat Generators (com carvão): sem eles nenhuma energia entra na máquina enquanto o chunk descarrega."""
    for name in MACHINES:
        for z in (PLAYER_Z + 1, SETBLOCK_Z + 1):
            if present:
                h.ctl("setblock", str(X[name]), "-60", str(z), GENERATOR)
            else:
                h.command(f"setblock {X[name]} -60 {z} minecraft:air")
    if present:
        h.ctl("wait", "10")
        for name in MACHINES:
            for z in (PLAYER_Z + 1, SETBLOCK_Z + 1):
                h.command(f'data merge block {X[name]} -60 {z} {{Items:[{{Slot:0b,id:"minecraft:coal",count:64}}]}}')


def gap():
    """O chunk fica carregado mas fora da distância de simulação (5 chunks) por 600 ticks, e outra máquina anda lá longe (a varredura dos objetos
    vivos corre quando uma máquina anda): o tile do Java continua o mesmo e a capacidade fica."""
    generators(False)
    zero()
    h.ctl("wait", "10")
    h.ctl("options-set", "simulationDistance", "5")
    h.ctl("render-distance", "12")
    away(70 + 140, PLAYER_Z, 600)
    print("longe:", h.ctl("block", str(X["enrichment_chamber"]), "-60", str(PLAYER_Z))[:90])
    back()
    h.ctl("wait", "40")
    generators(True)
    return read()


def unload():
    """O chunk descarrega de verdade (`block` responde `loaded: false`) e volta lido do disco. A máquina do jogador sem upgrade volta a 20 (32), a
    com 1 de velocidade a 1.341, como o rastro `placed_*` do Java."""
    generators(False)
    zero()
    h.ctl("wait", "10")
    away(70 + 4000, PLAYER_Z, 400, decoy=False)
    print("longe:", h.ctl("block", str(X["enrichment_chamber"]), "-60", str(PLAYER_Z))[:90])
    back()
    h.ctl("wait", "40")
    generators(True)
    expect = {name: CAPACITY[name][1] for name in MACHINES}
    expect["enrichment_chamber"] = 1341
    return read(expect)


def run():
    """Uma operação de 200 ticks de uma Enrichment Chamber posta por jogador: a energia entra de uma vez (1.000), a máquina gasta 5 por tick e
    para, como o rastro placed_run_enrichment_chamber_plain.jsonl (energia 500 e progresso 100 cem ticks depois, tudo gasto e a saída
    feita aos 200)."""
    x = X["enrichment_chamber"]
    pos = (x, -60, PLAYER_Z)
    for z in (PLAYER_Z + 1,):
        h.command(f"data merge block {x} -60 {z} {{Items:[]}}")
    h.ctl("wait", "5")
    h.command("tick freeze")
    h.command(f'data merge block {pos[0]} {pos[1]} {pos[2]} {{Items:[{{Slot:0b,id:"minecraft:clay",count:5}}],energy:1000}}')
    results = []
    total = 0
    for step in (1, 99, 100, 5):
        before = json.loads(h.ctl("state"))["ticks"]
        h.command(f"tick step {step}")
        for _ in range(100):
            time.sleep(0.3)
            if json.loads(h.ctl("state"))["ticks"] >= before + step:
                break
        total += step
        print("ticks andados:", total, "pelo jogo:", json.loads(h.ctl("state"))["ticks"] - (before + step - total))
        chat = h.command(f"data get block {pos[0]} {pos[1]} {pos[2]} progress")[0]
        energy = stored(pos)
        items = h.read_items(pos)
        results.append((step, energy, chat.split(": ")[-1], [(s, i, c) for (s, i, n, d, c) in items]))
        print(results[-1])
    h.command("tick unfreeze")


if __name__ == "__main__":
    stage = sys.argv[1]
    {"build": build, "upgrade": upgrade, "read": read, "drain": drain, "run": run, "gap": gap, "unload": unload}[stage]()
