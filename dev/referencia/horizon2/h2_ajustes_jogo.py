#!/usr/bin/env python3
"""Os ajustes dos cabos e dos canos no jogo (frente `ajustes-jogo`, Fase 3, decisões 133, 134 e 139 a 141).

    H_NAME=horizon PASTA=<pasta do game.log> h2_ajustes_jogo.py none cabo|cano   # o Configurator agachado no braço leste do transmissor do meio de uma linha de 3, e no oeste e no centro
    H_NAME=horizon PASTA=<pasta do game.log> h2_ajustes_jogo.py dimensoes        # o fim do tick em todas as dimensões: três cabos no Nether e três no overworld, tick a tick
    H_NAME=horizon PASTA=<pasta do game.log> h2_ajustes_jogo.py calor            # o Heat Generator entrega energia só pela frente, antes de produzir
    NOME=... h2_ajustes_jogo.py video                                            # o vídeo com som (o braço leste do cabo e do cano, e o Heat Generator)

A sonda (`TRANSMITTER_PROBE = true` na cópia de teste, `enviar_fontes.sh`) escreve no stderr do jogo (`PASTA/game.log`): `MBV {p, conn, kind, raw, net}` a cada vez que um cabo publica as conexões
dele, `MBW` o mesmo para um cano e `MBE` a energia de cada rede a cada `ServerTickEvent.Post`. O clique do Configurator só vale se o jogo mostrar o que o original mostra: cada clique
imprime o lado que mudou e o recorte do print do mesmo trecho, antes e depois, empilhados (`magick -append`).
"""
import json, os, subprocess, sys, time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
os.environ.setdefault("H_NAME", "horizon")
import h2_identity as h

PASTA = os.environ.get("PASTA", "~/.cache/rust-mekanism/restos")
BASE = (0, 150, 0)
KINDS = {
    "cabo": {"block": "mekanism:basic_universal_cable", "prefix": "MBV"},
    "cano": {"block": "mekanism:basic_mechanical_pipe", "prefix": "MBW"},
}
GENERATOR = "mekanismgenerators:heat_generator"
CHAMBER = "mekanism:enrichment_chamber"
SIDES = ["down", "up", "north", "south", "west", "east"]
SHOTS = os.environ.get("SHOTS", "~/.cache/rust-mekanism/ajustes-jogo-b1/prints")
LOCAL = os.environ.get("LOCAL", os.path.expanduser("~/.cache/rust-mekanism/wa/ajustes-jogo"))
# O recorte: o trecho do meio da linha, onde estão o braço do cabo do meio e o do vizinho.
CROP = os.environ.get("CROP", "900x420+510+330")


def cmd(text):
    try:
        return h.command(text)[0]
    except (KeyError, IndexError):
        return ""


def wait(n):
    h.ctl("wait", str(n))


def at(dx, dy=0, dz=0):
    return (BASE[0] + dx, BASE[1] + dy, BASE[2] + dz)


def txt(p):
    return "%d %d %d" % p


def clear():
    cmd("tick unfreeze")
    cmd("fill %d %d %d %d %d %d minecraft:stone" % (BASE[0] - 8, BASE[1] - 1, BASE[2] - 8, BASE[0] + 12, BASE[1] - 1, BASE[2] + 10))
    cmd("fill %d %d %d %d %d %d minecraft:air" % (BASE[0] - 8, BASE[1], BASE[2] - 8, BASE[0] + 12, BASE[1] + 6, BASE[2] + 10))
    wait(10)


def log_lines(prefix):
    out = subprocess.run(["ssh", "desktop", f"grep -a '^{prefix} ' {PASTA}/game.log | cut -c{len(prefix) + 2}-"], capture_output=True, text=True, timeout=120).stdout
    rows = []
    for line in out.splitlines():
        try:
            rows.append(json.loads(line))
        except ValueError:
            pass
    return rows


def last_views(prefix):
    last = {}
    for v in log_lines(prefix):
        last[tuple(v["p"])] = v
    return last


def shot(name):
    """Um print do jogo (o `horizon-ctl screenshot` grava dentro do container); copia para o notebook."""
    h.ctl("screenshot", f"/tmp/{name}.png")
    subprocess.run(["ssh", "desktop", f"mkdir -p {SHOTS} && docker exec {os.environ.get('H_NAME', 'horizon')} cat /tmp/{name}.png > {SHOTS}/{name}.png"], check=True)
    os.makedirs(LOCAL, exist_ok=True)
    subprocess.run(["scp", "-q", f"desktop:{SHOTS}/{name}.png", f"{LOCAL}/{name}.png"], check=True)
    return f"{LOCAL}/{name}.png"


def stack(before, after, out, label_before="antes", label_after="depois"):
    """Os dois prints recortados no mesmo trecho, um sobre o outro."""
    parts = []
    for name, label in ((before, label_before), (after, label_after)):
        cropped = f"{name}.corte.png"
        subprocess.run(["magick", name, "-crop", CROP, "+repage", "-gravity", "NorthWest", "-fill", "white", "-undercolor", "#00000099", "-pointsize", "28", "-annotate", "+8+8", label, cropped], check=True)
        parts.append(cropped)
    subprocess.run(["magick", *parts, "-append", out], check=True)
    for part in parts:
        os.remove(part)
    return out


def crop_diff(a, b):
    outs = []
    for name in (a, b):
        out = f"{name}.corte.png"
        subprocess.run(["magick", name, "-crop", CROP, "+repage", out], check=True)
        outs.append(out)
    result = subprocess.run(["compare", "-metric", "AE", outs[0], outs[1], "null:"], capture_output=True, text=True)
    for out in outs:
        os.remove(out)
    return int(float((result.stderr or "0").split()[0].split("(")[0]))


def camera(line=3):
    """A posição da câmera: ao norte da linha e um pouco acima, olhando o meio dela. É a mesma em todo print, para os recortes baterem."""
    mid = at(line / 2.0 - 0.5)
    return mid, (mid[0] + 0.5, mid[1] + 1.0, mid[2] - 2.6)


def enquadrar(line=3):
    mid, eye = camera(line)
    cmd("tp @s %.2f %.2f %.2f" % eye)
    wait(12)
    h.ctl("look-at", "%.2f" % (mid[0] + 0.5), "%.2f" % (mid[1] + 0.5), "%.2f" % (mid[2] + 0.5))
    wait(6)


def looking():
    """O que a mira do jogador toca (o `looking-at` do jogo): bloco, face e ponto, em coordenadas do bloco."""
    block = json.loads(h.ctl("looking-at")).get("block") or {}
    pos = block.get("pos")
    point = block.get("point")
    local = [round(point[i] - pos[i], 3) for i in range(3)] if pos and point else None
    return pos, block.get("face"), local


def click_at(pos, point, stand=None, sneak=True):
    """O clique do Configurator agachado de quem olha o ponto `point` (em coordenadas do bloco `pos`) como um jogador: a mira é a do `look-at` e o clique é o do botão direito do mouse, e o
    servidor decide o braço pela mira (`getSideLookingAt`). Devolve onde a mira caiu (o bloco, a face e o ponto)."""
    if stand is not None:
        cmd("tp @s %.2f %.2f %.2f" % stand)
        wait(12)
    h.ctl("select", "mekanism:configurator")
    if sneak:
        h.ctl("key", "shift", "down")
        wait(6)
    h.ctl("look-at", *("%.3f" % (pos[i] + point[i]) for i in range(3)))
    wait(6)
    aimed = looking()
    h.ctl("mouse", "right", "click")
    wait(8)
    if sneak:
        h.ctl("key", "shift", "up")
    time.sleep(1.2)
    return aimed


ARM = {"east": (0.85, 0.5, 0.5), "west": (0.15, 0.5, 0.5), "centro": (0.5, 0.5, 0.5)}


def snapshot(prefix, line):
    """O tipo cru e o braço de cada lado de cada transmissor da linha, na última vista que cada um publicou."""
    views = last_views(prefix)
    return {x: (views.get(at(x)) or {"raw": [None] * 6, "conn": [None] * 6}) for x in range(line)}


def changes(old, new):
    """O que mudou entre duas fotos da linha: o tipo cru de um lado (`x<n>.<lado>: antes -> depois`) e o braço dele que veio ou foi."""
    out = []
    for x in old:
        for i, side in enumerate(SIDES):
            if old[x]["raw"][i] != new[x]["raw"][i]:
                out.append(f"x{x}.{side}: {old[x]['raw'][i]} -> {new[x]['raw'][i]}")
            if old[x]["conn"][i] != new[x]["conn"][i]:
                out.append(f"x{x}.{side}: braço {old[x]['conn'][i]} -> {new[x]['conn'][i]}")
    return out


def none(kind, line=3):
    """O `NONE` que o usuário viu: o Configurator agachado no braço leste do transmissor do meio de uma linha de 3 cicla `PUSH`, `PULL`, `NONE` (o braço do meio e o do vizinho somem)
    e volta a `NORMAL` na face. Depois o braço oeste e o centro. A cada clique: onde a mira caiu, o lado que mudou (da vista que o transmissor publicou) e o print."""
    spec = KINDS[kind]
    prefix = spec["prefix"]
    clear()
    cmd("gamemode creative @s")
    cmd("clear @s")
    h.ctl("options-set", "chatVisibility", "2")
    for x in range(line):
        cmd("setblock %s %s" % (txt(at(x)), spec["block"]))
    wait(40)
    h.ctl("give", "mekanism:configurator", "1")
    middle = at(1)
    _, eye = camera(line)
    ok = True
    enquadrar(line)
    antes = shot(f"{kind}_antes")
    controle = shot(f"{kind}_antes_2")
    print("ruído entre dois prints iguais:", crop_diff(antes, controle), "pixels")
    state = snapshot(prefix, line)

    def click(side, label, expect):
        nonlocal state, ok
        aimed = click_at(middle, ARM[side], eye)
        new = snapshot(prefix, line)
        done = changes(state, new)
        enquadrar(line)
        time.sleep(1.0)
        picture = shot(f"{kind}_{label}")
        good = done == expect
        ok &= good
        print(("OK   " if good else "DIFF ") + f"{label}: a mira caiu em {aimed}; mudou {done} (esperado {expect}); recorte {crop_diff(antes, picture)} pixels diferentes do print de antes")
        state = new
        return picture

    click("east", "leste_push", ["x1.east: normal -> push"])
    click("east", "leste_pull", ["x1.east: push -> pull"])
    none_picture = click("east", "leste_none", ["x1.east: pull -> none", "x1.east: braço True -> False", "x2.west: braço True -> False"])
    print("empilhado:", stack(antes, none_picture, f"{LOCAL}/{kind}_antes_depois_none.png", "antes (NORMAL)", "depois (NONE no leste)"))
    # Sem braço não há o que mirar: o clique cai na face leste do centro e o lado clicado volta a `NORMAL`; o vizinho sai para a face ficar à vista, e volta (o braço do meio e o do vizinho com ele).
    cmd("setblock %s minecraft:air" % txt(at(2)))
    wait(10)
    aimed = click_at(middle, (0.69, 0.5, 0.5), (middle[0] + 3.0, middle[1] + 0.5, middle[2] + 0.5))
    cmd("setblock %s %s" % (txt(at(2)), spec["block"]))
    wait(30)
    time.sleep(1.5)
    enquadrar(line)
    depois = shot(f"{kind}_leste_normal")
    new = snapshot(prefix, line)
    good = new[1]["raw"][5] == "normal" and bool(new[1]["conn"][5]) and bool(new[2]["conn"][4])
    ok &= good
    print(("OK   " if good else "DIFF ") + f"leste_normal: a mira caiu em {aimed}; a face clicada voltou a {new[1]['raw'][5]}, braços {new[1]['conn'][5]} {new[2]['conn'][4]}; recorte {crop_diff(antes, depois)} pixels diferentes do print de antes")
    state = new
    # O braço oeste: o oeste muda, o leste não.
    click("west", "oeste_push", ["x1.west: normal -> push"])
    # O centro: o raio acerta o centro, `getSideLookingAt` é nulo e o lado que muda é o clicado (o norte).
    click("centro", "centro", ["x1.north: normal -> push"])
    print("fim", "OK" if ok else "DIFF")
    return ok


def nbt(p, key, dim=None):
    """Um valor do NBT do block entity de `p` (`data get block`), como número, ou `None` se a chave não existe ou o bloco não está carregado."""
    prefix = f"execute in {dim} run " if dim else ""
    out = cmd(f"{prefix}data get block {txt(p)} {key}")
    if "following block data:" not in out:
        return None
    text = out.split("following block data:")[1].strip()
    try:
        return int(text.rstrip("bLsd"))
    except ValueError:
        return None


def step(n=1):
    """`tick step n` e a espera de `0.35 + 0.05 n` s (o servidor anda a 20 ticks por segundo depois de o comando voltar)."""
    cmd(f"tick step {n}")
    time.sleep(0.35 + 0.05 * n)


def mbe_count():
    return len(log_lines("MBE"))


def dimensoes():
    """O `ServerTickPost` em todas as dimensões (decisão 134): três cabos universais, um cubo de energia criativo na frente e uma Enrichment Chamber no fim, no Nether (um chunk
    forçado, ninguém lá) e no overworld (onde o jogador está), com `tick freeze` e `tick step 1`. A energia da máquina do Nether tem de andar igual à do overworld, tick a tick (antes da
    correção ficava em zero); a sonda `MBE` imprime uma linha por dimensão com rede a cada passada. Depois o chunk deixa de ser forçado e a máquina do Nether para."""
    nether = "minecraft:the_nether"
    origins = {None: (0, 150, 0), nether: (0, 100, 0)}

    def scene(dim):
        o = origins[dim]
        prefix = f"execute in {dim} run " if dim else ""
        cmd(prefix + "fill %d %d %d %d %d %d minecraft:stone" % (o[0] - 3, o[1] - 1, o[2] - 3, o[0] + 6, o[1] - 1, o[2] + 3))
        cmd(prefix + "fill %d %d %d %d %d %d minecraft:air" % (o[0] - 3, o[1], o[2] - 3, o[0] + 6, o[1] + 3, o[2] + 3))
        wait(10)
        for x in range(3):
            cmd(prefix + "setblock %d %d %d mekanism:basic_universal_cable" % (o[0] + 1 + x, o[1], o[2]))
        cmd(prefix + "setblock %d %d %d mekanism:creative_energy_cube" % (o[0], o[1], o[2]))
        cmd(prefix + "setblock %d %d %d mekanism:enrichment_chamber" % (o[0] + 4, o[1], o[2]))
        wait(30)
        cmd(prefix + "data merge block %d %d %d {component_config:{config0:[I;4,4,4,4,4,4],eject0:1b},energy_amount:9223372036854775807L}" % o)
        cmd(prefix + "data merge block %d %d %d {component_config:{config0:[I;1,1,1,1,1,1],config3:[I;1,1,1,1,1,1]},Items:[{Slot:0b,id:\"minecraft:iron_ore\",count:64}]}" % (o[0] + 4, o[1], o[2]))

    cmd("tick unfreeze")
    cmd("gamemode creative @s")
    cmd("tp @s 2.5 151 -4.5")
    cmd(f"execute in {nether} run forceload add 0 0")
    wait(40)
    for dim in origins:
        scene(dim)
    wait(60)
    chamber = {dim: (o[0] + 4, o[1], o[2]) for dim, o in origins.items()}
    cmd("tick freeze")
    time.sleep(1.0)
    before = mbe_count()
    rows = []
    for t in range(1, 41):
        step(1)
        rows.append((t, nbt(chamber[None], "energy"), nbt(chamber[nether], "energy", nether), nbt(chamber[None], "progress"), nbt(chamber[nether], "progress", nether)))
    time.sleep(1.0)
    lines = log_lines("MBE")[before:]
    per_pass = {}
    for line in lines:
        if line["runs"] and line["nets"]:
            per_pass[line["t"]] = per_pass.get(line["t"], 0) + 1
    # As duas máquinas começam em instantes diferentes (a do overworld ticou enquanto a cena do Nether se montava): compara a energia e quanto o progresso andou a cada passo.
    same = all(r[1] == r[2] for r in rows) and all(rows[i][3] - rows[i - 1][3] == rows[i][4] - rows[i - 1][4] == 1 for i in range(1, len(rows)))
    print(f"as passadas que rodaram normalmente e tinham rede: {len(per_pass)}")
    moved = any((r[1] or 0) > 0 or (r[3] or 0) > 0 for r in rows)
    for r in rows[:8] + rows[-3:]:
        print("   t=%d energia overworld %s nether %s; progresso overworld %s nether %s" % r)
    print(("OK   " if same and moved else "DIFF ") + f"a máquina do Nether anda como a do overworld nos 40 passos (a energia igual e o progresso andando 1 por passo nas duas: {same}; saiu de zero: {moved})")
    both = sum(1 for n in per_pass.values() if n >= 2)
    print(("OK   " if both >= 38 else "DIFF ") + f"a sonda MBE: {len(per_pass)} passadas que rodaram com rede, {both} delas com uma linha por dimensão (40 passos)")
    # Sem o chunk forçado e sem ninguém no Nether, nada anda lá.
    cmd(f"execute in {nether} run forceload remove 0 0")
    cmd("tick unfreeze")
    time.sleep(8.0)
    cmd("tick freeze")
    loaded = nbt(chamber[nether], "progress", nether)
    stopped = loaded == rows[-1][4] or loaded is None
    print(("OK   " if stopped else "DIFF ") + f"sem o chunk forçado e sem ninguém no Nether, nada anda lá: progresso {loaded} (o último passo: {rows[-1][4]}; None: o chunk descarregou)")
    cmd("tick unfreeze")
    return same and moved and both >= 38 and stopped


def calor(frente="east"):
    """O Heat Generator virado para o leste com lava, um cabo na frente (e uma Enrichment Chamber no fim dele) e uma Enrichment Chamber em cada um dos outros quatro lados (oeste, norte,
    sul, cima): só a energia do que está ligado à frente chega (o `ENERGY_SIDES` do gerador é `FRONT`). Lê a energia e o progresso de cada máquina a cada tick."""
    clear()
    cmd("gamemode creative @s")
    cmd("clear @s")
    g = at(0, 0, 0)
    spots = {"frente (cabo)": at(2, 0, 0), "oeste": at(-1, 0, 0), "norte": at(0, 0, -1), "sul": at(0, 0, 1), "cima": at(0, 1, 0)}
    cmd("setblock %s %s[facing=east]" % (txt(g), GENERATOR))
    cmd("setblock %s mekanism:basic_universal_cable" % txt(at(1, 0, 0)))
    for p in spots.values():
        cmd("setblock %s %s" % (txt(p), CHAMBER))
    wait(30)
    for p in spots.values():
        cmd("data merge block %s {component_config:{config0:[I;1,1,1,1,1,1],config3:[I;1,1,1,1,1,1]},Items:[{Slot:0b,id:\"minecraft:iron_ore\",count:64}]}" % txt(p))
    wait(20)
    cmd("tp @s 0.5 152 -4.5")
    cmd("data merge block %s {machinery_basis:{v:1,tanks:[{resource:\"minecraft:lava\",amount:1000L}]}}" % txt(g))
    cmd("tick freeze")
    time.sleep(1.0)
    seen = {name: [] for name in spots}
    for t in range(1, 61):
        step(1)
        if t % 10 == 0:
            cmd("data merge block %s {machinery_basis:{v:1,tanks:[{resource:\"minecraft:lava\",amount:1000L}]}}" % txt(g))
        for name, p in spots.items():
            seen[name].append((nbt(p, "energy") or 0, nbt(p, "progress") or 0))
    for name, values in seen.items():
        print("   %-14s energia máxima %6d, progresso máximo %3d" % (name, max(v[0] for v in values), max(v[1] for v in values)))
    front = max(v[0] for v in seen["frente (cabo)"]) > 0 or max(v[1] for v in seen["frente (cabo)"]) > 0
    others = all(max(v[0] for v in values) == 0 and max(v[1] for v in values) == 0 for name, values in seen.items() if name != "frente (cabo)")
    print(("OK   " if front and others else "DIFF ") + f"só a máquina da frente (por um cabo) recebe: frente {front}, os outros lados sem nada {others}")
    cmd("tick unfreeze")
    return front and others


def montar_video():
    """A cena do vídeo: três cabos em `z 0` (x 0 a 2), três canos em `z 5`, e o Heat Generator virado para o leste em `x 10` com um cabo na frente e uma Enrichment Chamber no fim dele e uma em cada um
    dos outros quatro lados, com lava."""
    cmd("tick unfreeze")
    cmd("fill %d %d %d %d %d %d minecraft:stone" % (BASE[0] - 6, BASE[1] - 1, BASE[2] - 8, BASE[0] + 18, BASE[1] - 1, BASE[2] + 10))
    cmd("fill %d %d %d %d %d %d minecraft:air" % (BASE[0] - 6, BASE[1], BASE[2] - 8, BASE[0] + 18, BASE[1] + 8, BASE[2] + 10))
    wait(10)
    cmd("gamemode creative @s")
    cmd("clear @s")
    for x in range(3):
        cmd("setblock %s %s" % (txt(at(x, 0, 0)), KINDS["cabo"]["block"]))
        cmd("setblock %s %s" % (txt(at(x, 0, 5)), KINDS["cano"]["block"]))
    cmd("setblock %s %s[facing=east]" % (txt(at(10, 0, 0)), GENERATOR))
    cmd("setblock %s mekanism:basic_universal_cable" % txt(at(11, 0, 0)))
    for p in (at(12, 0, 0), at(9, 0, 0), at(10, 0, -1), at(10, 0, 1), at(10, 1, 0)):
        cmd("setblock %s %s" % (txt(p), CHAMBER))
    wait(30)
    for p in (at(12, 0, 0), at(9, 0, 0), at(10, 0, -1), at(10, 0, 1), at(10, 1, 0)):
        cmd("data merge block %s {component_config:{config0:[I;1,1,1,1,1,1],config3:[I;1,1,1,1,1,1]},Items:[{Slot:0b,id:\"minecraft:iron_ore\",count:64}]}" % txt(p))
    wait(20)
    h.ctl("give", "mekanism:configurator", "1")
    h.ctl("options-set", "chatVisibility", "2")


def olhar(eye, alvo):
    cmd("tp @s %.2f %.2f %.2f" % eye)
    wait(12)
    h.ctl("look-at", *("%.2f" % c for c in alvo))
    wait(6)


def roteiro_video(pausa):
    """O que o vídeo mostra: o Configurator agachado no braço leste do cabo do meio (PUSH, PULL, NONE: o braço some, e o do vizinho com ele) e de volta a NORMAL na face; o mesmo num cano; e
    o Heat Generator com lava entregando energia só à máquina da frente (por um cabo), com as outras quatro paradas. A mira fica no braço entre um clique e outro (cada chamada do
    `horizon-ctl` leva cerca de um segundo, e o vídeo tem 60)."""
    for kind, dz in (("cabo", 0), ("cano", 5)):
        spec = KINDS[kind]
        middle = at(1, 0, dz)
        eye = (middle[0] + 0.5, middle[1] + 1.0, middle[2] - 2.6)
        centro = (middle[0] + 0.5, middle[1] + 0.5, middle[2] + 0.5)
        olhar(eye, centro)
        pausa(1.0)
        h.ctl("key", "shift", "down")
        h.ctl("look-at", *("%.3f" % (middle[i] + ARM["east"][i]) for i in range(3)))
        wait(6)
        for want in ("push", "pull", "none"):
            h.ctl("mouse", "right", "click")
            wait(20)
            pausa(0.8)
        h.ctl("key", "shift", "up")
        olhar(eye, centro)
        pausa(1.0)
        cmd("setblock %s minecraft:air" % txt(at(2, 0, dz)))
        cmd("tp @s %.2f %.2f %.2f" % (middle[0] + 3.0, middle[1] + 0.5, middle[2] + 0.5))
        wait(10)
        h.ctl("key", "shift", "down")
        h.ctl("look-at", *("%.3f" % (middle[i] + (0.69, 0.5, 0.5)[i]) for i in range(3)))
        wait(6)
        h.ctl("mouse", "right", "click")
        wait(8)
        h.ctl("key", "shift", "up")
        cmd("setblock %s %s" % (txt(at(2, 0, dz)), spec["block"]))
        olhar(eye, centro)
        pausa(0.8)
    g = at(10, 0, 0)
    olhar((g[0] + 1.5, g[1] + 3.0, g[2] - 5.0), (g[0] + 1.5, g[1] + 0.5, g[2] + 0.5))
    cmd("data merge block %s {machinery_basis:{v:1,tanks:[{resource:\"minecraft:lava\",amount:1000L}]}}" % txt(g))
    for i in range(3):
        pausa(2.5)
        cmd("data merge block %s {machinery_basis:{v:1,tanks:[{resource:\"minecraft:lava\",amount:1000L}]}}" % txt(g))
    pausa(0.5)


def video():
    """O vídeo com som (`NOME`, `SEGUNDOS`, `ARQUIVO`): a cena do `h2_cabo_jogo.video`, com o roteiro dos ajustes."""
    import h2_cabo_jogo as c
    c.montar_cena = montar_video
    c.video_roteiro = roteiro_video
    c.video()


def main():
    what = sys.argv[1] if len(sys.argv) > 1 else "none"
    if what == "none":
        sys.exit(0 if none(sys.argv[2] if len(sys.argv) > 2 else "cabo") else 1)
    if what == "dimensoes":
        sys.exit(0 if dimensoes() else 1)
    if what == "video":
        video()
        return
    if what == "cena":
        montar_video()
        roteiro_video(lambda t: time.sleep(t))
        return
    if what == "calor":
        sys.exit(0 if calor() else 1)
    raise SystemExit("uso: h2_ajustes_jogo.py none cabo|cano | dimensoes | calor | video")


if __name__ == "__main__":
    main()
