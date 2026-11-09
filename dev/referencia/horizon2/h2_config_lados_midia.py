#!/usr/bin/env python3
"""Prints e vídeo da configuração de lados (`H_NAME=horizon`): quatro Crushers virados para os quatro lados, com um baú em cada face.
  capturar PASTA_NO_DESKTOP PASTA_LOCAL   tira os prints (passo_NN.png) no desktop, e guarda a legenda de cada um (medida dos baús) em PASTA_LOCAL
  montar PASTA SAIDA.mp4   junta os prints com a legenda (ffmpeg, no notebook, com os prints já copiados para PASTA)
"""
import json, os, re, subprocess, sys, time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import h2_config_lados as c

ROW = [(70 + 4 * i, -60, 130, facing) for i, facing in enumerate(["north", "east", "south", "west"])]
SHOTS = os.path.expanduser("~/.local/share/jogo-isolado/horizon/data/shots")


def scene(config, ejecting, gravel=20):
    c.command("tick freeze")
    for x, y, z, facing in ROW:
        for face in c.FACES:
            c.setblock(c.at((x, y, z), face), "minecraft:air")
        c.setblock((x, y, z), "minecraft:air")
    time.sleep(0.4)
    for x, y, z, facing in ROW:
        for face in c.FACES:
            c.setblock(c.at((x, y, z), face), "minecraft:chest")
        c.setblock((x, y, z), f"{c.MACHINE}[facing={facing}]")
    time.sleep(0.8)
    for x, y, z, facing in ROW:
        c.command(f'data merge block {x} {y} {z} {{Items:[{{Slot:1b,id:"{c.GRAVEL}",count:{gravel}}}],' + c.config_nbt(item=c.sides_with(config), eject=ejecting) + "}")


def where():
    """Para cada máquina, os lados do mundo cujo baú tem cascalho."""
    out = []
    for x, y, z, facing in ROW:
        out.append("/".join(f[0].upper() if f not in ("up", "down") else f[0].upper() for f in c.FACES if c.total(c.at((x, y, z), f), c.GRAVEL) > 0) or "nenhum")
    return out


def shot(caption, desk, local, index):
    c.command("tp @s 76.5 -56.5 121 0 14")
    time.sleep(0.6)
    name = f"passo_{index:02d}"
    c.ctl("screenshot", f"{desk}/{name}.png")
    open(f"{local}/{name}.txt", "w").write(caption)
    print(index, caption)


def capturar(desk, local):
    os.makedirs(local, exist_ok=True)
    subprocess.run(["ssh", "desktop", f"mkdir -p {desk}"], check=True)
    c.command("gamemode creative @s")
    c.ctl("key", "f1", "tap")
    scene({}, True)
    c.step(60)
    shot(f"Posto por setblock: tudo em NONE, nada sai. Baús com cascalho: {where()}", desk, local, 0)
    scene({"right": 4}, True)
    c.step(60)
    shot(f"Direita em OUTPUT: sai só pelo baú da direita de cada frente (N, E, S, W). Baús: {where()}", desk, local, 1)
    scene({"right": 4, "back": 4, "bottom": 4}, True)
    c.step(60)
    shot(f"Direita, costas e base em OUTPUT: a pilha vai toda para a base (D). Baús: {where()}", desk, local, 2)
    scene({"right": 4}, False)
    c.step(60)
    shot(f"Ejetar desligado: nada sai. Baús: {where()}", desk, local, 3)
    c.ctl("key", "f1", "tap")


def montar(folder, saida):
    arquivos = sorted(f for f in os.listdir(folder) if f.startswith("passo_") and f.endswith(".png"))
    fonte = "/usr/share/fonts/noto/NotoSans-Regular.ttf"
    legendadas = []
    for arquivo in arquivos:
        destino = os.path.join(folder, "leg_" + arquivo)
        filtro = f"drawbox=x=0:y=0:w=iw:h=70:color=black@0.75:t=fill,drawtext=fontfile={fonte}:textfile={os.path.join(folder, arquivo[:-4] + '.txt')}:fontcolor=white:fontsize=26:x=(w-text_w)/2:y=22"
        subprocess.run(["ffmpeg", "-v", "error", "-y", "-i", os.path.join(folder, arquivo), "-vf", filtro, destino], check=True)
        legendadas.append(destino)
    lista = os.path.join(folder, "lista.txt")
    with open(lista, "w") as f:
        for destino in legendadas:
            f.write(f"file '{destino}'\nduration 4\n")
        f.write(f"file '{legendadas[-1]}'\n")
    subprocess.run(["ffmpeg", "-v", "error", "-y", "-f", "concat", "-safe", "0", "-i", lista, "-vf", "fps=30,format=yuv420p", "-c:v", "libx264", "-crf", "22", "-movflags", "+faststart", saida], check=True)


if __name__ == "__main__":
    if sys.argv[1] == "capturar":
        capturar(sys.argv[2], sys.argv[3])
    else:
        montar(sys.argv[2], sys.argv[3])
