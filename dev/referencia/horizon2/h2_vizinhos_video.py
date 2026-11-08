#!/usr/bin/env python3
"""O ciclo do `updatePower` pela vizinhança no container `horizon` (H_NAME=horizon), em prints e um vídeo com legenda.

    H_NAME=horizon h2_vizinhos_video.py capturar [DIRETORIO]     monta, anda e tira os prints (1280x720)
    h2_vizinhos_video.py montar DIRETORIO SAIDA.mp4              onde houver `ffmpeg` (com `drawtext`): legenda cada passo e junta

A Enrichment Chamber em HIGH, com energia e argila: o caso comum (a alavanca liga e desliga a máquina) e os que ler a cada tick e ler por aviso fazem diferente (o bloco de
redstone posto sem avisar nada deixa a máquina parada; o que sai sem avisar a deixa andando). O servidor fica parado e anda com `tick step`. Os valores de `redstone` das
legendas são os do rastro do original (`dev/referencia/rastros/neighbors_chamber_unnotified.jsonl`, linha 0 = HIGH), que `h2_vizinhos.py chamber_unnotified` confere no jogo, tick a tick.
"""
import json, os, re, subprocess, sys, time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import h2_identity as h

SHOTS = "/home/luka/.local/share/jogo-isolado/horizon/data/shots/vizinhos"
MACHINE = (10, 150, 40)
SOURCE = (9, 150, 40)
POKE = (11, 150, 40)
CAMERA = (10.5, 150.0, 44.5)
LOCAL = os.path.expanduser("~/.cache/rust-mekanism/wa/vizinhos")

STEPS = [
    ("parada", "1. Enrichment Chamber em HIGH, sem sinal: parada (redstone = false)"),
    ("alavanca_on", "2. Alavanca ligada (avisa os vizinhos): redstone = true, a máquina anda"),
    ("alavanca_off", "3. Alavanca desligada: redstone = false, a máquina para"),
    ("redstone_sem_aviso", "4. Bloco de redstone posto sem avisar (setblock strict): encosta nela, mas redstone segue false"),
    ("poke_avisa", "5. Um bloco posto ao lado avisa a máquina: ela lê o sinal agora, redstone = true"),
    ("sai_sem_aviso", "6. O bloco de redstone sai sem avisar: redstone segue true e a máquina segue andando"),
    ("poke_sai_avisa", "7. O outro bloco sai e avisa: redstone = false, a máquina para"),
]


def at(p):
    return " ".join(map(str, p))


def step(ticks):
    h.command(f"tick step {ticks}")
    time.sleep(0.35 + 0.05 * ticks)


STATE = []


def read_state():
    """O que o NBT da máquina diz agora: `redstone`, `active_state` e `progress` (o port os escreve no tick, o `step` já andou)."""
    text = h.command(f"data get block {at(MACHINE)}")[0]
    red = re.search(r"redstone: (\d)b", text)
    active = re.search(r"active_state: (\d)b", text)
    progress = re.search(r"progress: (\d+)", text)
    return {"redstone": bool(int(red.group(1))), "active": bool(int(active.group(1))), "progress": int(progress.group(1))}


def shot(out, n, name):
    time.sleep(0.8)
    h.ctl("screenshot", f"{out}/passo_{n:02d}_{name}.png")
    STATE.append(read_state())


def capturar(out):
    # A pasta é do desktop (o jogo roda lá e grava o print no caminho que recebe).
    subprocess.run(["ssh", "desktop", f"mkdir -p {out}"], check=True)
    x, y, z = MACHINE
    h.command("gamemode creative")
    h.command("tick unfreeze")
    h.ctl("close")
    h.command("time set noon")
    h.command("weather clear")
    h.command(f"tp @s {CAMERA[0]} {CAMERA[1] + 6} {CAMERA[2]}")
    h.ctl("wait", "10")
    h.command(f"fill {x - 4} {y - 1} {z - 4} {x + 4} {y - 1} {z + 8} minecraft:stone")
    h.command(f"fill {x - 4} {y} {z - 4} {x + 4} {y + 4} {z + 8} minecraft:air")
    h.command(f"tp @s {CAMERA[0]} {CAMERA[1]} {CAMERA[2]} 180 12")
    h.ctl("wait", "10")
    h.command("tick freeze")
    h.command(f"setblock {at(MACHINE)} mekanism:enrichment_chamber[facing=south]{{control_type:\"high\",energy:200000,Items:[{{Slot:0b,id:\"minecraft:clay\",count:64}}]}}")
    h.command(f"setblock {at(SOURCE)} minecraft:lever[face=floor,facing=north,powered=false]")
    step(8)
    shot(out, 0, STEPS[0][0])
    h.ctl("interact", *map(str, SOURCE))
    step(12)
    shot(out, 1, STEPS[1][0])
    h.ctl("interact", *map(str, SOURCE))
    step(70)
    shot(out, 2, STEPS[2][0])
    h.command(f"setblock {at(SOURCE)} minecraft:redstone_block strict")
    step(12)
    shot(out, 3, STEPS[3][0])
    h.command(f"setblock {at(POKE)} minecraft:stone")
    step(12)
    shot(out, 4, STEPS[4][0])
    h.command(f"setblock {at(SOURCE)} minecraft:air strict")
    step(12)
    shot(out, 5, STEPS[5][0])
    h.command(f"setblock {at(POKE)} minecraft:air")
    step(70)
    shot(out, 6, STEPS[6][0])
    h.command(f"fill {x - 4} {y - 1} {z - 4} {x + 4} {y + 4} {z + 8} minecraft:air")
    h.command("tick unfreeze")
    with open(os.path.join(LOCAL, "estado.json"), "w") as f:
        json.dump(STATE, f)


def legenda(n):
    texto = STEPS[n][1]
    estado = json.load(open(os.path.join(LOCAL, "estado.json")))[n]
    return f"{texto}\nNBT da máquina: redstone={str(estado['redstone']).lower()}, ativa={str(estado['active']).lower()}, progresso={estado['progress']}"


def montar(frames, saida):
    arquivos = sorted(f for f in os.listdir(frames) if f.startswith("passo_") and f.endswith(".png"))
    if len(arquivos) != len(STEPS):
        raise SystemExit(f"{len(arquivos)} passos em {frames}, esperava {len(STEPS)}")
    fonte = "/usr/share/fonts/noto/NotoSans-Regular.ttf"
    legendadas = []
    for n, arquivo in enumerate(arquivos):
        texto = legenda(n)
        destino = os.path.join(frames, "leg_" + arquivo)
        arquivo_texto = os.path.join(frames, "leg_" + arquivo + ".txt")
        with open(arquivo_texto, "w") as f:
            f.write(texto)
        filtro = f"drawbox=x=0:y=0:w=iw:h=96:color=black@0.75:t=fill,drawtext=fontfile={fonte}:textfile={arquivo_texto}:fontcolor=white:fontsize=24:x=(w-text_w)/2:y=12"
        subprocess.run(["ffmpeg", "-v", "error", "-y", "-i", os.path.join(frames, arquivo), "-vf", filtro, destino], check=True)
        legendadas.append(destino)
    lista = os.path.join(frames, "lista.txt")
    with open(lista, "w") as f:
        for destino in legendadas:
            f.write(f"file '{destino}'\nduration 3.5\n")
        f.write(f"file '{legendadas[-1]}'\n")
    subprocess.run(["ffmpeg", "-v", "error", "-y", "-f", "concat", "-safe", "0", "-i", lista, "-vf", "fps=30,format=yuv420p", "-c:v", "libx264", "-crf", "22", "-movflags", "+faststart", saida], check=True)


if __name__ == "__main__":
    if sys.argv[1] == "capturar":
        capturar(sys.argv[2] if len(sys.argv) > 2 else SHOTS)
    elif sys.argv[1] == "montar":
        montar(sys.argv[2], sys.argv[3])
