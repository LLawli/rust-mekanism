#!/usr/bin/env python3
"""Os 22 transmissores (os quatro cabos, os tubos, os condutores, os canos, os transportadores, o restritivo e o desviador) em jogo, desenhados pelo OBJ do original (decisão 26).

Uso (`H_NAME=horizon` ou `horizon2`, o jogo aberto num mundo plano, o jogador criativo, a janela de 1920 por 1080):
- estados: põe cada transmissor, sozinho e ligado a um vizinho da própria família, e lê de volta o bloco: o estado tem de ter o lado ligado e nenhum modelo ausente no log do jogo;
- fileira: os 22 em uma fileira (as famílias se ligam entre si, uma família não se liga à outra), com as gamerules do relógio e do clima paradas, o meio-dia e o céu limpo;
- video SAIDA.mp4 [SEGUNDOS] [CONTAINER]: grava, pela captura PipeWire do gamescope (`gravar-jogo rec`, veja `procedures/gravar-video-real-do-jogo.md`), a câmera passando pela fileira
  de longe e de perto. Com o container, o som do jogo vai junto (sink nulo próprio); sem ele, o vídeo é mudo;
- devolver: devolve as gamerules e limpa a fileira.
"""
import os, subprocess, sys, time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import h2_identity as h

TIERS = ("basic", "advanced", "elite", "ultimate")
FAMILIES = ("universal_cable", "pressurized_tube", "thermodynamic_conductor", "mechanical_pipe", "logistical_transporter")
IDS = [f"{t}_{f}" for f in FAMILIES for t in TIERS] + ["restrictive_transporter", "diversion_transporter"]
Y, Z, X0 = -60, 82, 60
GRAVADOR = "~/.cache/rust-mekanism/gravar-jogo"
GAMERULES = ("advance_time", "advance_weather", "spawn_mobs")


def command(text):
    return h.command(text)


def clear_noon():
    """O céu limpo e o meio-dia, com os ciclos parados (o pedido do usuário de 2026-11-11 para todo print e vídeo)."""
    for text in ("difficulty peaceful", "gamerule spawn_mobs false", "gamerule advance_time false", "gamerule advance_weather false", "time set noon", "weather clear", "kill @e[type=!minecraft:player]"):
        command(text)


def row():
    command(f"fill {X0 - 2} {Y} {Z - 3} {X0 + len(IDS) + 2} {Y + 6} {Z + 3} minecraft:air")
    for i, name in enumerate(IDS):
        command(f"setblock {X0 + i} {Y} {Z} mekanism:{name}")


def estados():
    row()
    time.sleep(1.0)
    ok = True
    for i, name in enumerate(IDS):
        got = h.ctl("block", str(X0 + i), str(Y), str(Z))
        state = h.json.loads(got)["state"] if hasattr(h, "json") else got
        print(name, state)
        ok &= name in state
    return ok


def video(out, seconds, container):
    clear_noon()
    row()
    time.sleep(2.0)
    command("gamemode creative")
    last = X0 + len(IDS)
    path = [(X0 - 2, last + 1, 7.0, 7.8), (last + 1, X0 - 2, 3.4, 12.0)]
    gravar = f"{GRAVADOR} rec transmissor-obj {out} {seconds}" + (f" {container}" if container else "")
    # A câmera começa antes da gravação; a captura dura `seconds`, e os passos de câmera cabem nela.
    first = path[0]
    command(f"tp @s {first[0]}.0 {Y + 1.62 + 0.0:.2f} {Z - first[2]:.2f} 0 {first[3]}")
    time.sleep(2.0)
    proc = subprocess.Popen(["ssh", "desktop", gravar])
    time.sleep(1.5)
    per_leg = (seconds - 3.0) / len(path)
    for start, end, dist, pitch in path:
        steps = max(int(per_leg / 0.2), 1)
        for step in range(steps + 1):
            x = start + (end - start) * step / steps
            command(f"tp @s {x:.2f} {Y + 1.62 + 0.0:.2f} {Z - dist:.2f} 0 {pitch}")
            time.sleep(0.2)
    proc.wait()


def devolver():
    for rule in GAMERULES:
        command(f"gamerule {rule} true")
    command(f"fill {X0 - 2} {Y} {Z - 3} {X0 + len(IDS) + 2} {Y + 6} {Z + 3} minecraft:air")


def main():
    mode = sys.argv[1]
    if mode == "estados":
        sys.exit(0 if estados() else 1)
    if mode == "fileira":
        clear_noon()
        row()
    elif mode == "video":
        video(sys.argv[2], int(sys.argv[3]) if len(sys.argv) > 3 else 30, sys.argv[4] if len(sys.argv) > 4 else "")
    elif mode == "devolver":
        devolver()


if __name__ == "__main__":
    main()
