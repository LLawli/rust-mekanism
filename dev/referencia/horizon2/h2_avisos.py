#!/usr/bin/env python3
"""Os avisos de entrada no container `horizon` (H_NAME=horizon): um print por passo com a tela da máquina aberta e o servidor parado, e um vídeo com legenda.

    H_NAME=horizon h2_avisos.py capturar [DIRETORIO]     monta, anda e tira os prints (guiScale 3)
    h2_avisos.py montar DIRETORIO SAIDA.mp4 [LEGENDAS.txt]   onde houver `ffmpeg` (com `drawtext`): legenda cada passo e junta

`NOT_ENOUGH_INPUT` e `NOT_ENOUGH_SECONDARY_INPUT` nunca acendem (`dev/referencia/testes/WarningsTraceTest.java`: a entrada que não basta é receita que não
casa, sem erro), então o ciclo mostra o que acontece de verdade: a entrada que não basta não acende nada, o resto chega e a máquina anda, e o aviso que acende
(o de energia) acende e apaga. A receita é a de `processing/iron/dust/from_raw_ore` (3 de ferro bruto) na Enrichment Chamber e a de terra com 10 de bio no Infuser.
Entre uma mudança do NBT e o print anda 2 ticks: o `data merge` zera os data slots da tela até o tick seguinte.
"""
import os, subprocess, sys, time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import h2_identity as h

SHOTS = "/home/luka/.local/share/jogo-isolado/horizon/data/shots/avisos"
SPOT = (70, -60, 100)
STAND = (70, -60, 96)

STEPS = [
    ("ferro_2", "1. Enrichment Chamber com 2 de ferro bruto (a receita pede 3) e energia: nada acende, a seta não anda"),
    ("ferro_3", "2. Chega o terceiro: a receita casa e a seta anda (20 ticks)"),
    ("sem_energia", "3. Sem energia: o aviso de energia acende, a barra pisca e a aba de avisos aparece"),
    ("energia_volta", "4. A energia volta: o aviso apaga e a seta segue"),
    ("bio_9", "5. Metallurgic Infuser com 9 de bio (a receita pede 10): nada acende, a barra de químico não pisca"),
    ("bio_10", "6. Com 10 a receita casa e a máquina anda"),
    ("bio_8", "7. O tanque cai abaixo de 10 no meio da operação: o progresso volta a zero, sem aviso"),
]


def pos(p=SPOT):
    return " ".join(map(str, p))


def step(ticks):
    """`tick step N` volta antes de os N ticks andarem: espera os 50 ms de cada um antes de mexer ou ler."""
    h.command(f"tick step {ticks}")
    time.sleep(0.35 + 0.05 * ticks)


def shot(out, n, name):
    time.sleep(0.6)
    h.ctl("screenshot", f"{out}/passo_{n:02d}_{name}.png")


def place(block):
    h.command("tick unfreeze")
    h.ctl("close")
    h.command("difficulty peaceful")
    h.command("gamemode creative")
    h.ctl("tp", *map(str, STAND))
    h.ctl("wait", "20")
    h.command(f"setblock {pos()} minecraft:air")
    time.sleep(0.5)
    h.command("clear @s")
    h.ctl("give", block, "1")
    out = h.ctl("place", *map(str, SPOT), "--item", block)
    assert '"error"' not in out, out
    h.command("clear @s")
    h.ctl("wait", "10")
    h.command("tick freeze")
    step(3)


def open_menu():
    h.ctl("tp", *map(str, STAND))
    for _ in range(4):
        h.ctl("interact", *map(str, SPOT))
        time.sleep(0.8)
        if '"screen":"container"' in h.ctl("screen").replace(" ", ""):
            h.ctl("look", "0", "0")
            return
    raise SystemExit("o menu não abriu")


def merge(body):
    h.command(f"data merge block {pos()} {{{body}}}")


def capturar(out):
    os.makedirs(out, exist_ok=True)
    h.ctl("options-set", "guiScale", "3")
    h.ctl("options-set", "chatVisibility", "2")
    place("mekanism:enrichment_chamber")
    merge('energy:1000,Items:[{Slot:0b,id:"minecraft:raw_iron",count:2}]')
    step(3)
    open_menu()
    step(20)
    shot(out, 0, "ferro_2")
    merge('Items:[{Slot:0b,id:"minecraft:raw_iron",count:3}]')
    step(22)
    shot(out, 1, "ferro_3")
    merge("energy:0")
    step(3)
    shot(out, 2, "sem_energia")
    merge("energy:1000")
    step(12)
    shot(out, 3, "energia_volta")
    h.ctl("close")
    place("mekanism:metallurgic_infuser")
    merge('energy:1000,Items:[{Slot:1b,id:"minecraft:dirt",count:2}],chemical_tanks:[{container:0b,stored:{type:"mekanism:bio",amount:9L}}]')
    step(3)
    open_menu()
    step(20)
    shot(out, 4, "bio_9")
    merge('chemical_tanks:[{container:0b,stored:{type:"mekanism:bio",amount:10L}}]')
    step(22)
    shot(out, 5, "bio_10")
    merge('chemical_tanks:[{container:0b,stored:{type:"mekanism:bio",amount:8L}}]')
    step(3)
    shot(out, 6, "bio_8")
    h.ctl("close")
    h.command(f"setblock {pos()} minecraft:air")
    h.command("tick unfreeze")


def montar(frames, saida, legendas=None):
    arquivos = sorted(f for f in os.listdir(frames) if f.startswith("passo_") and f.endswith(".png"))
    if len(arquivos) != len(STEPS):
        raise SystemExit(f"{len(arquivos)} passos em {frames}, esperava {len(STEPS)}")
    fonte = "/usr/share/fonts/noto/NotoSans-Regular.ttf"
    legendadas = []
    for arquivo, (_, texto) in zip(arquivos, STEPS):
        destino = os.path.join(frames, "leg_" + arquivo)
        arquivo_texto = os.path.join(frames, "leg_" + arquivo + ".txt")
        with open(arquivo_texto, "w") as f:
            f.write(texto)
        filtro = f"drawbox=x=0:y=0:w=iw:h=70:color=black@0.75:t=fill,drawtext=fontfile={fonte}:textfile={arquivo_texto}:fontcolor=white:fontsize=32:x=(w-text_w)/2:y=17"
        subprocess.run(["ffmpeg", "-v", "error", "-y", "-i", os.path.join(frames, arquivo), "-vf", filtro, destino], check=True)
        legendadas.append(destino)
    lista = os.path.join(frames, "lista.txt")
    with open(lista, "w") as f:
        for destino in legendadas:
            f.write(f"file '{destino}'\nduration 3.2\n")
        f.write(f"file '{legendadas[-1]}'\n")
    subprocess.run(["ffmpeg", "-v", "error", "-y", "-f", "concat", "-safe", "0", "-i", lista, "-vf", "fps=30,format=yuv420p", "-c:v", "libx264", "-crf", "22", "-movflags", "+faststart", saida], check=True)


if __name__ == "__main__":
    modo = sys.argv[1] if len(sys.argv) > 1 else ""
    if modo == "capturar":
        capturar(sys.argv[2] if len(sys.argv) > 2 else SHOTS + "/ciclo")
    elif modo == "montar" and len(sys.argv) >= 4:
        montar(sys.argv[2], sys.argv[3])
    else:
        raise SystemExit(__doc__)
