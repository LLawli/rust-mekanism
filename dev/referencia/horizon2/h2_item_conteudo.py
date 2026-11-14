#!/usr/bin/env python3
"""Em jogo (container `horizon2`, H_NAME=horizon2): o conteúdo guardado no item da máquina (frente `item-conteudo`, Fase 5).

    h2_item_conteudo.py [DIRETORIO_DOS_PRINTS]
    h2_item_conteudo.py video        o roteiro do vídeo (uma Enrichment Chamber com upgrades e itens: a tela aberta antes, quebrada, o tooltip do item, posta de novo e a tela aberta depois); grave com
                                     `gravar-jogo rec item-conteudo cena.mp4 50 <container>` aberto uns 2 s antes
    h2_item_conteudo.py efeito       o efeito dos upgrades do item depois de posto (o item com 2 de velocidade, posto por jogador: a energia e o progresso de tick em tick contra `placed_run_<máquina>_speed2.jsonl`)

Para cada máquina (a Enrichment Chamber, o Crusher, o Energized Smelter, o Metallurgic Infuser e o Heat Generator): dá o item com os componentes que o original solta
(`mekanism:upgrades`, `items`, `chemicals` ou `fluids` e `heat`, `redstone_control`, `energy`), põe o bloco, confere o que o tile guardou (`component_upgrade`, `items`, o tanque, o
modo de redstone), quebra com a picareta e confere o item solto (os componentes e nenhum item solto a mais no chão), e põe o item solto de volta. Cada máquina tira um print do
tooltip do item na tela do inventário. A saída é `legendas.json` na pasta. O jogo aberto num mundo, em criativo, sem som e headless (regra de builds e jogo).
"""
import json, os, re, subprocess, sys, time

TOOLS = os.path.dirname(os.path.abspath(__file__))
os.environ.setdefault("H_NAME", "horizon2")
NAME = os.environ["H_NAME"]
SHOTS_HOST = f"/home/luka/.local/share/jogo-isolado/{NAME}/data/shots/item_conteudo"
# Dentro do container o caminho é sempre o do `horizon`.
SHOTS = "/home/luka/.local/share/jogo-isolado/horizon/data/shots/item_conteudo"
OUT = sys.argv[1] if len(sys.argv) > 1 and sys.argv[1] not in ("efeito", "video") else os.path.expanduser("~/.cache/rust-mekanism/wa/item-conteudo")
captions = []
failures = []

sys.path.insert(0, TOOLS)
import h2_identity as hid  # noqa: E402
from item_conteudo_dados import MACHINES  # noqa: E402


def ctl(*args):
    return subprocess.run([f"{TOOLS}/ctl.sh", *args], capture_output=True, text=True, timeout=180).stdout


def cmd(text):
    try:
        return " ".join(json.loads(ctl("command", text))["chat"])
    except (json.JSONDecodeError, KeyError):
        return ""


def widgets():
    return json.loads(ctl("ui"))


def shot(name, caption):
    time.sleep(0.8)
    ctl("screenshot", f"{SHOTS}/{len(captions):02d}_{name}.png")
    captions.append({"file": f"{len(captions):02d}_{name}.png", "caption": caption})
    print(caption)


def expect(label, ok, detail=""):
    print(("ok   " if ok else "FALHOU"), label, detail if not ok else "")
    if not ok:
        failures.append(label)


def main():
    os.makedirs(OUT, exist_ok=True)
    subprocess.run(["ssh", "desktop", f"rm -rf {SHOTS_HOST}; mkdir -p {SHOTS_HOST}"], check=True)
    ctl("wait-ready")
    time.sleep(5)
    cmd("gamemode creative")
    cmd("tp @s 4 102 -4 0 20")
    cmd("fill -4 100 -8 16 100 10 minecraft:stone")
    cmd("fill -4 101 -8 16 106 10 minecraft:air")
    cmd("kill @e[type=minecraft:item]")
    cmd("clear @s")
    for item, pos, components, upgrade_label, upgrade_total in MACHINES:
        short = item.split(":")[1]
        cmd(f"setblock {pos[0]} {pos[1]} {pos[2]} minecraft:air")
        cmd("clear @s")
        cmd(f"give @s {item}[{components}]")
        ctl("select", "1")
        # o tooltip do item com o conteúdo, na tela do inventário
        cmd("gamemode survival")
        ctl("open-inventory")
        time.sleep(1.0)
        slots = [w for w in widgets().get("widgets", []) if w.get("kind") == "slot" and (w.get("stack") or {}).get("item") == item]
        if slots:
            x, y, w, h = slots[0]["bounds"]
            ctl("scroll-at", str(int(x + 8)), str(int(y + 8)), "0")
            shot(f"tooltip_{short}", f"Tooltip do item {short} com upgrades, itens, tanque e modo de redstone")
        else:
            failures.append(f"sem slot de {item}")
        ctl("close")
        cmd("gamemode creative")
        ctl("select", "1")
        ctl("tp", str(pos[0]), "101", str(pos[2] + 3))
        ctl("select", "1")
        ctl("tp", str(pos[0]), "101", str(pos[2] + 3))
        print(ctl("place", *map(str, pos), "--item", item))
        time.sleep(1.5)
        tile = cmd("data get block {} {} {}".format(*pos))
        expect(f"{short}: o tile guarda os upgrades do item", f'"mekanism:speed": {upgrade_total}' in tile or f'mekanism:speed: {upgrade_total}' in tile or f"{upgrade_label.lower()}" in tile.lower(), tile[:400])
        expect(f"{short}: o tile guarda o modo de redstone", "high" in tile, tile[:400])
        expect(f"{short}: o tile guarda os itens", ("cobblestone" in tile) or ("coal" in tile), tile[:400])
        cmd("tp @s 4 102 -4 0 20")
        shot(f"posta_{short}", f"{short} posta pelo item: o tile traz o conteúdo")
        # quebrar em sobrevivência com a picareta
        cmd("gamemode survival")
        cmd("give @s minecraft:diamond_pickaxe")
        ctl("select", "minecraft:diamond_pickaxe")
        cmd("kill @e[type=minecraft:item]")
        ctl("tp", str(pos[0]), "101", str(pos[2] + 3))
        print(ctl("break", *map(str, pos)))
        time.sleep(1.2)
        item_nbt = cmd("data get entity @e[type=minecraft:item,limit=1,sort=nearest] Item")
        expect(f"{short}: o item solto leva os upgrades", "mekanism:upgrades" in item_nbt, item_nbt[:400])
        expect(f"{short}: o item solto leva os itens", "mekanism:items" in item_nbt, item_nbt[:400])
        expect(f"{short}: o item solto leva o modo de redstone", "mekanism:redstone_control" in item_nbt, item_nbt[:400])
        if "infuser" in short:
            expect(f"{short}: o item solto leva o tanque", "mekanism:chemicals" in item_nbt, item_nbt[:400])
        if "heat" in short:
            expect(f"{short}: o item solto leva a lava e o calor", "mekanism:fluids" in item_nbt and "mekanism:heat" in item_nbt, item_nbt[:400])
        shot(f"quebrada_{short}", f"{short} quebrada: o item solto leva o conteúdo e nada cai no chão")
        # nenhum item solto a mais: o `kill` conta o que está no chão (a máquina e mais nada), e a pedra, o ferro, o carvão e a redstone ficaram no item
        killed = cmd("kill @e[type=minecraft:item]")
        expect(f"{short}: só o item da máquina está no chão", killed.startswith("Killed ") and "entities" not in killed, killed)
        # repor o item solto
        cmd("gamemode creative")
        cmd("clear @s mekanism:" + short.replace("mekanismgenerators:", ""))
        cmd(f"setblock {pos[0]} {pos[1]} {pos[2]} minecraft:air")
        cmd("kill @e[type=minecraft:item]")
    json.dump(captions, open(os.path.join(OUT, "legendas.json"), "w"), ensure_ascii=False, indent=1)
    print("falhas:", failures)

# Os ticks depois da carga de energia que o rastro `placed_run_*_speed2.jsonl` tem na linha `t = 3 + k`: o consumo por tick, o progresso e a saída pronta.
EFEITO = {
    "enrichment_chamber": ("clay", 0, 1, 1792),
    "crusher": ("cobblestone", 0, 1, 2912),
    "energized_smelter": ("sand", 0, 1, 1792),
}
EFEITO_PASSOS = [0, 1, 2, 10, 50, 100, 105, 110, 111, 112, 113, 120]
SPEED2 = 'mekanism:upgrades={upgrades:{"mekanism:speed":2},input:{},output:{}}'


def lido(pos, out_slot):
    energy = progress = None
    chat = cmd("data get block {} {} {} energy".format(*pos))
    found = re.search(r"(-?\d+)\D*$", chat)
    if found:
        energy = int(found.group(1))
    chat = cmd("data get block {} {} {} progress".format(*pos))
    found = re.search(r"(-?\d+)\D*$", chat)
    if found:
        progress = int(found.group(1))
    output = ["", 0]
    for slot, id_, _name, _data, count in hid.read_items(pos):
        if slot == out_slot:
            output = [id_, count]
    return energy, progress, output


def efeito():
    """Uma operação de cada máquina posta de um item com 2 de velocidade (o item que o original solta), de tick em tick, contra o rastro do original."""
    rastros = os.path.join(TOOLS, "..", "rastros")
    ctl("wait-ready")
    cmd("gamemode creative")
    cmd("fill -4 100 -8 16 100 10 minecraft:stone")
    cmd("fill -4 101 -8 16 106 10 minecraft:air")
    for index, (name, (entrada, in_slot, out_slot, burst)) in enumerate(EFEITO.items()):
        item = f"mekanism:{name}"
        pos = (2 + 4 * index, 101, 0)
        rows = {}
        for line in open(os.path.join(rastros, f"placed_run_{name}_speed2.jsonl")):
            row = json.loads(line)
            if "t" in row:
                rows[row["t"]] = row
        cmd("tick unfreeze")
        cmd(f"setblock {pos[0]} {pos[1]} {pos[2]} minecraft:air")
        cmd("clear @s")
        cmd(f"give @s {item}[{SPEED2}]")
        ctl("select", "1")
        ctl("tp", str(pos[0]), "101", str(pos[2] + 3))
        print(ctl("place", *map(str, pos), "--item", item).strip())
        time.sleep(1.5)
        cmd("tick freeze")
        cmd(f'data merge block {pos[0]} {pos[1]} {pos[2]} {{Items:[{{Slot:{in_slot}b,id:"minecraft:{entrada}",count:5}}]}}')
        cmd(f"data merge block {pos[0]} {pos[1]} {pos[2]} {{energy:{burst}}}")
        done = 0
        bad = 0
        for target in EFEITO_PASSOS:
            if target > done:
                before = json.loads(ctl("state"))["ticks"]
                cmd(f"tick step {target - done}")
                for _ in range(200):
                    time.sleep(0.3)
                    if json.loads(ctl("state"))["ticks"] >= before + target - done:
                        break
                done = target
            energy, progress, output = lido(pos, out_slot)
            want = rows[3 + target]
            same = energy == want["energy"] and progress == want["progress"] and output == want["output"]
            bad += not same
            print(f"{name} k={target:3d} port energia={energy} progresso={progress} saida={output} | original energia={want['energy']} progresso={want['progress']} saida={want['output']} {'ok' if same else 'DIFERE'}")
        expect(f"{name}: a máquina posta do item com velocidade 2 anda como o original ({len(EFEITO_PASSOS)} marcas)", bad == 0)
        cmd(f"setblock {pos[0]} {pos[1]} {pos[2]} minecraft:air")
    cmd("tick unfreeze")
    print("falhas:", failures)


def video():
    """O roteiro do vídeo, com pausas para a tela se ver. A máquina é a primeira de `MACHINES` (a Enrichment Chamber com 3 de velocidade, 2 de energia, três pilhas e modo de redstone)."""
    import h2_blocos as hb

    item, _pos, components, _label, _total = MACHINES[0]
    x, y, z = 60, -60, 80
    stand = f"{x + 0.5} {y} {z + 3.5} 180 20"
    ctl("wait-ready")
    for c in ("gamemode survival", "weather clear", "time set noon", "gamerule advance_time false", "gamerule advance_weather false", "gamerule spawn_mobs false", "kill @e[type=!minecraft:player]",
              f"fill {x - 8} {y} {z - 8} {x + 8} {y + 6} {z + 8} minecraft:air", "clear @s"):
        cmd(c)
    ctl("options-set", "guiScale", "4")
    ctl("options-set", "chatVisibility", "2")
    cmd(f"tp @s {stand}")
    time.sleep(1.0)
    # antes: o item posto e a tela aberta, com a aba de upgrades
    hb.item_cena(f"item replace entity @a hotbar.0 with {item}[{components}]")
    hb.item_cena(f"#colocar {x} {y - 1} {z}")
    time.sleep(1.5)
    hb.item_cena(f"#abrir {x} {y} {z}")
    time.sleep(3.0)
    hb.item_cena("#clicar 190,19")
    time.sleep(4.0)
    hb.item_cena("#tela")
    time.sleep(1.0)
    # quebrada: o item cai, e a mão o pega
    hb.item_cena(f"#quebrar {x} {y} {z}")
    time.sleep(2.0)
    cmd("execute as @a at @e[type=minecraft:item,limit=1] run tp @a ~ ~ ~")
    time.sleep(2.0)
    cmd(f"tp @s {stand}")
    time.sleep(1.0)
    # o tooltip do item que a mão pegou
    hb.item_cena("#inventario 0")
    time.sleep(3.0)
    # a tecla de detalhes (Left Shift) mostra os detalhes do item, como no original
    ctl("key", "shift", "down")
    time.sleep(4.0)
    ctl("key", "shift", "up")
    hb.item_cena("#tela")
    time.sleep(1.0)
    # posta de novo, e a tela depois
    hb.item_cena(f"#colocar {x} {y - 1} {z}")
    time.sleep(1.5)
    hb.item_cena(f"#abrir {x} {y} {z}")
    time.sleep(3.0)
    hb.item_cena("#clicar 190,19")
    time.sleep(5.0)
    hb.item_cena("#tela")
    hb.item_cena(f"#quebrar {x} {y} {z}")
    time.sleep(1.0)
    cmd("kill @e[type=minecraft:item]")
    cmd("clear @s")


if len(sys.argv) > 1 and sys.argv[1] == "efeito":
    efeito()
elif len(sys.argv) > 1 and sys.argv[1] == "video":
    video()
else:
    main()
