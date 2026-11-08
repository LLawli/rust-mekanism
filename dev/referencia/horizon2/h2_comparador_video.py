#!/usr/bin/env python3
"""Prints e vídeos do comparador das máquinas no container `horizon` (H_NAME=horizon), com a legenda de cada passo.

    H_NAME=horizon h2_comparador_video.py capturar ordem|cinco|posta [DIRETORIO]   monta o cenário, anda e tira os prints (1280x720)
    h2_comparador_video.py montar ordem|cinco|posta DIRETORIO SAIDA.mp4            onde houver `ffmpeg` (com `drawtext`): legenda cada passo e junta

- `ordem`: duas Enrichment Chambers encostadas em duas ordens de ticker (a linha de cima põe N antes de S, a de baixo S antes de N), S em HIGH com o `redstone`
  velho (bloco de redstone posto sem avisar). N recebe itens, o nível do comparador dela muda no tick seguinte e N avisa S dentro do próprio tick: S só anda no mesmo
  tick onde tica depois de N.
- `cinco`: as cinco máquinas, cada uma com um comparador a leste, vazias, com itens e cheias (lava no Heat Generator); a legenda traz a saída de cada comparador.
- `posta`: uma máquina posta por um jogador ao lado de um Heat Generator cheio (240): ela já tem a capacidade do item (1.000) no tick da colocação e recebe os 240.
Os valores das legendas são lidos do NBT e dos comparadores no jogo; os do original estão em `dev/referencia/rastros/comparator_*` e `placed_fed_*`, que `h2_comparador.py`
e `h2_placed_fed.py` conferem tick a tick.
"""
import json, os, re, subprocess, sys, time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import h2_identity as h
import h2_comparador as c

SHOTS = "/home/luka/.local/share/jogo-isolado/horizon/data/shots/comparador"
LOCAL = os.path.expanduser("~/.cache/rust-mekanism/wa/comparador")
SLOT_CLAY = f'{{Slot:0b,id:"{c.CLAY}",count:64}}'
MACHINES = ["enrichment_chamber", "crusher", "energized_smelter", "metallurgic_infuser", "heat_generator"]
INPUT_SLOT = {"enrichment_chamber": 0, "crusher": 0, "energized_smelter": 0, "metallurgic_infuser": 1, "heat_generator": None}


def camera(x, y, z, yaw, pitch):
    """Leva o jogador (em espectador, sem gravidade nem mão na tela) para onde a câmera fica. O servidor parado não anda o `tp`: desfaz o freeze, espera e para de novo."""
    h.command("tick unfreeze")
    h.command("gamemode spectator")
    h.command(f"tp @s {x} {y} {z} {yaw} {pitch}")
    h.ctl("wait", "20")
    h.command("tick freeze")


def shot(out, n, name, captions, store):
    time.sleep(0.8)
    h.ctl("screenshot", f"{out}/passo_{n:02d}_{name}.png")
    store.append(captions())


def prepare_view(rows):
    c.prepare(rows, comparators=2)
    x0, y0, z0 = c.BASE
    camera(x0 + 4.5, y0 + 10, z0 + 3.0 * rows + 1.0, 0, 90)


# ---- ordem ----


def reading_order():
    lines = []
    for row in range(2):
        s, n = c.read_nbt(c.pos_of("S", row)), c.read_nbt(c.pos_of("N", row))
        cn, cs = c.read_output(c.abs_pos((4, 1, c.STRIDE * row + 2))), c.read_output(c.abs_pos((4, 1, c.STRIDE * row + 3)))
        order = "N tica antes de S" if row == 0 else "S tica antes de N"
        lines.append(
            f"linha {row} ({order}): S ativa={str(s['active']).lower()} progresso={s['progress']} | nível de N={n['comparator']} | comparadores N={cn} S={cs}"
        )
    return "\n".join(lines)


def capture_order(out):
    state = {"fresh": {}}
    prepare_view(2)
    captions = []
    for row, order in ((0, "NS"), (1, "SN")):
        for who in order:
            c.apply_order(f"put {row} {who} 3", state)
    for row in range(2):
        c.apply_order(f"fill {row}", state)
        c.apply_order(f"src {row} on 2", state)
    c.step(8)
    shot(out, 0, "estado_inicial", reading_order, captions)
    for row in range(2):
        c.apply_order(f"ninput {row}", state)
    shot(out, 1, "itens_em_N", reading_order, captions)
    c.step(1)
    shot(out, 2, "tick_1", reading_order, captions)
    c.step(1)
    shot(out, 3, "tick_2", reading_order, captions)
    c.step(3)
    shot(out, 4, "comparadores", reading_order, captions)
    return captions


ORDER_TITLES = [
    "1. S em HIGH com o redstone velho (bloco posto sem avisar): parada",
    "2. Itens postos em N (ainda sem tick)",
    "3. N muda o nível do comparador e avisa os vizinhos dentro do tick dela: a linha de cima já anda",
    "4. O tick seguinte: a linha de baixo (S tica antes de N) só agora anda",
    "5. Dois ticks depois da mudança, os comparadores mostram o nível novo",
]


# ---- cinco ----


def place_five(extra=None):
    for k, name in enumerate(MACHINES):
        p = c.abs_pos((3, 1, 2 + 3 * k))
        block = {"heat_generator": "mekanismgenerators:heat_generator"}.get(name, f"mekanism:{name}")
        nbt = '{control_type:"high"}' if name == "heat_generator" else ""
        h.command(f"setblock {c.text(p)} {block}{nbt}")
        c.comparator(c.abs_pos((4, 1, 2 + 3 * k)))


def set_contents(contents):
    for k, name in enumerate(MACHINES):
        p = c.abs_pos((3, 1, 2 + 3 * k))
        spec = contents[name]
        if name == "heat_generator":
            lava = spec
            tank = f'{{resource:"minecraft:lava",amount:{lava}L}}' if lava else "{amount:0L}"
            h.command(f"data merge block {c.text(p)} {{machinery_basis:{{v:1,tanks:[{tank}]}}}}")
        else:
            item = {"crusher": "minecraft:cobblestone", "energized_smelter": "minecraft:sand", "metallurgic_infuser": "minecraft:dirt"}.get(name, c.CLAY)
            items = ",".join(f'{{Slot:{slot}b,id:"{item}",count:{n}}}' for slot, n in spec)
            h.command(f"data merge block {c.text(p)} {{Items:[{items}]}}")


def reading_five():
    out = []
    for k, name in enumerate(MACHINES):
        out.append(f"{name}: {c.read_output(c.abs_pos((4, 1, 2 + 3 * k)))}")
    return "\n".join(out)


def capture_five(out):
    c.prepare(5, comparators=1)
    x0, y0, z0 = c.BASE
    h.command("fill {} {} {} {} {} {} minecraft:stone".format(x0, y0 - 1, z0 - 2, x0 + 12, y0 - 1, z0 + 20))
    place_five()
    camera(x0 + 4.5, y0 + 9, z0 + 8.5, 0, 90)
    captions = []
    plans = [
        ("vazias", {m: ([] if m != "heat_generator" else 0) for m in MACHINES}),
        ("entrada_cheia", {m: ([(INPUT_SLOT[m], 64)] if m != "heat_generator" else 500) for m in MACHINES}),
        ("entrada_e_saida", {m: ([(INPUT_SLOT[m], 64), (INPUT_SLOT[m] + 1, 64)] if m != "heat_generator" else 999) for m in MACHINES}),
        ("cheias", {m: ([(i, 64) for i in range(4 if m == "metallurgic_infuser" else 3)] if m != "heat_generator" else 1000) for m in MACHINES}),
    ]
    for n, (name, contents) in enumerate(plans):
        set_contents(contents)
        c.step(5)
        shot(out, n, name, reading_five, captions)
    return captions


FIVE_TITLES = [
    "1. As cinco máquinas vazias: comparadores em 0",
    "2. Entrada com 64 itens (e o tanque de lava do Heat Generator com 500): o Heat Generator lê a lava, as outras a média dos slots",
    "3. Entrada e saída com 64 (lava 999)",
    "4. Todos os slots cheios (lava 1000): 15",
]


# ---- posta ----


def reading_posta():
    x0, y0, z0 = c.BASE
    generator, machine = (x0 + 2, y0, z0 + 2), (x0 + 3, y0, z0 + 2)
    out = []
    for label, p in (("Heat Generator", generator), ("máquina", machine)):
        nbt = c.read_nbt(p)
        out.append(f"{label}: energia={None if nbt is None else nbt['energy']}")
    return "\n".join(out)


def capture_posta(out):
    c.prepare(1, comparators=0)
    x0, y0, z0 = c.BASE
    machine, generator = (x0 + 3, y0, z0 + 2), (x0 + 2, y0, z0 + 2)
    block = h.BLOCK["enrichment_chamber"]
    camera(x0 + 3.5, y0 + 6, z0 + 2.5, 0, 90)
    h.command(f"setblock {c.text(generator)} mekanismgenerators:heat_generator[facing=east]{{energy:240}}")
    c.step(3)
    h.command(f"data merge block {c.text(generator)} {{energy:240}}")
    captions = []
    shot(out, 0, "gerador_cheio", reading_posta, captions)
    h.command("tick unfreeze")
    h.command("gamemode creative")
    h.command("clear @s")
    h.ctl("give", block, "1")
    h.command(f"tp @s {machine[0]} {machine[1] + 3} {machine[2]}")
    h.ctl("wait", "10")
    h.command("tick freeze")
    result = h.ctl("place", str(machine[0]), str(machine[1]), str(machine[2]), "--item", block)
    assert '"error"' not in result, result
    camera(x0 + 3.5, y0 + 6, z0 + 2.5, 0, 90)
    c.step(1)
    shot(out, 1, "tick_da_colocacao", reading_posta, captions)
    c.step(3)
    shot(out, 2, "tres_ticks_depois", reading_posta, captions)
    return captions


POSTA_TITLES = [
    "1. Heat Generator cheio (240) de frente para o lugar da máquina",
    "2. Um jogador põe a Enrichment Chamber e o primeiro tick passa: a capacidade do item (1.000) já vale, e os 240 entram de uma vez (com 20 seriam 20)",
    "3. Três ticks depois: nada mudou, a máquina guarda os 240",
]

SCENES = {
    "ordem": (capture_order, ORDER_TITLES),
    "cinco": (capture_five, FIVE_TITLES),
    "posta": (capture_posta, POSTA_TITLES),
}


def capturar(scene, out):
    subprocess.run(["ssh", "desktop", f"mkdir -p {out}"], check=True)
    h.command("gamemode creative")
    h.command("tick unfreeze")
    h.ctl("close")
    h.command("time set noon")
    h.command("weather clear")
    captions = SCENES[scene][0](out)
    os.makedirs(LOCAL, exist_ok=True)
    with open(os.path.join(LOCAL, f"legendas_{scene}.json"), "w") as f:
        json.dump(captions, f)
    x0, y0, z0 = c.BASE
    h.command(f"fill {x0} {y0} {z0 - 2} {x0 + 12} {y0 + 8} {z0 + 24} minecraft:air")
    h.command("tick unfreeze")


def montar(scene, frames, saida):
    titles = SCENES[scene][1]
    arquivos = sorted(f for f in os.listdir(frames) if f.startswith("passo_") and f.endswith(".png"))
    if len(arquivos) != len(titles):
        raise SystemExit(f"{len(arquivos)} passos em {frames}, esperava {len(titles)}")
    captions = json.load(open(os.path.join(LOCAL, f"legendas_{scene}.json")))
    fonte = "/usr/share/fonts/noto/NotoSans-Regular.ttf"
    legendadas = []
    for n, arquivo in enumerate(arquivos):
        destino = os.path.join(frames, "leg_" + arquivo)
        arquivo_texto = os.path.join(frames, "leg_" + arquivo + ".txt")
        with open(arquivo_texto, "w") as f:
            f.write(titles[n] + "\n" + captions[n])
        filtro = (
            "drawbox=x=0:y=0:w=iw:h=150:color=black@0.75:t=fill,"
            f"drawtext=fontfile={fonte}:textfile={arquivo_texto}:fontcolor=white:fontsize=22:x=(w-text_w)/2:y=12:line_spacing=6"
        )
        subprocess.run(["ffmpeg", "-v", "error", "-y", "-i", os.path.join(frames, arquivo), "-vf", filtro, destino], check=True)
        legendadas.append(destino)
    lista = os.path.join(frames, "lista.txt")
    with open(lista, "w") as f:
        for destino in legendadas:
            f.write(f"file '{destino}'\nduration 4\n")
        f.write(f"file '{legendadas[-1]}'\n")
    subprocess.run(
        ["ffmpeg", "-v", "error", "-y", "-f", "concat", "-safe", "0", "-i", lista, "-vf", "fps=30,format=yuv420p", "-c:v", "libx264", "-crf", "22", "-movflags", "+faststart", saida],
        check=True,
    )


if __name__ == "__main__":
    if sys.argv[1] == "capturar":
        capturar(sys.argv[2], sys.argv[3] if len(sys.argv) > 3 else f"{SHOTS}/{sys.argv[2]}")
    elif sys.argv[1] == "montar":
        montar(sys.argv[2], sys.argv[3], sys.argv[4])
