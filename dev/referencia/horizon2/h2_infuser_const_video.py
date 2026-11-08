#!/usr/bin/env python3
"""Grava o ciclo completo do Metallurgic Infuser com a receita de `per_tick_usage` no container `horizon` (H_NAME=horizon): um print por passo,
com a tela da máquina aberta e o servidor parado, e confere os números do ciclo. Duas etapas, com o jogo salvo e reaberto no meio:

    H_NAME=horizon h2_infuser_const_video.py antes [DIRETORIO]    monta, anda, seca o tanque, devolve o químico e salva (depois: `horizon-ctl quit` e abra o jogo de novo)
    H_NAME=horizon h2_infuser_const_video.py depois [DIRETORIO]   lê o mundo reaberto, confere o estado salvo e termina a operação
    h2_infuser_const_video.py montar DIRETORIO SAIDA.mp4          onde houver `ffmpeg` (com `drawtext`): legenda cada passo e junta

O ciclo (receita: gravel e 2 de redstone por tick, 200 ticks): 10 ticks (progresso 10, tanque 980); 100 (tanque 800, a 2 por tick); a primeira
operação pronta (1 flint, tanque 600, o último tick gasta 2 a mais); o tanque a zero no meio da segunda (o progresso para em 50); o químico de volta
(o progresso segue); salvar e sair; reabrir (progresso, tanque e `used_so_far` iguais); a segunda operação pronta (2 flints, tanque 200).
"""
import json, os, subprocess, sys, time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import h2_identity as h
import h2_infuser_const as c

SHOTS = "/home/luka/.local/share/jogo-isolado/horizon/data/shots/infuser-const"
STATE = os.path.join(os.environ.get("TMPDIR", "/tmp"), "infuser_const_estado.json")

STEPS = [
    ("progresso_10", "1. Gravel e 1000 de redstone: a operação roda, e o tanque cai 2 por tick (progresso 10 de 200, tanque 980)"),
    ("meio", "2. Meio da operação: progresso 100 de 200, tanque 800"),
    ("primeira_pronta", "3. Primeira operação pronta: 1 flint, tanque 600 (o tick final gasta o resto dos 400)"),
    ("tanque_vazio", "4. Tanque a zero no meio da segunda: o progresso para em 50 e a receita espera, sem zerar"),
    ("quimico_volta", "5. O químico volta: o progresso segue de 50 (aqui 75), o tanque cai 2 por tick"),
    ("antes_de_salvar", "6. Antes de salvar e sair: progresso 75, tanque 450"),
    ("reaberto", "7. Mundo reaberto: progresso 75 e tanque 450, iguais ao que foi salvo"),
    ("segunda_pronta", "8. Segunda operação pronta: 2 flints e tanque 200"),
]


def step(ticks):
    """Anda os ticks com o carvão dos geradores reposto: a energia não pode faltar no meio."""
    c.fuel()
    c.step(ticks)


def shot(out, n, name):
    time.sleep(0.6)
    h.ctl("screenshot", f"{out}/passo_{n:02d}_{name}.png")


def open_menu():
    for _ in range(4):
        h.ctl("interact", *map(str, c.SPOT))
        time.sleep(0.8)
        if '"screen":"container"' in h.ctl("screen").replace(" ", ""):
            return
    raise SystemExit("o menu não abriu")


def antes(out):
    h.ctl("options-set", "guiScale", "3")
    c.build()
    c.apply("item gravel 3, tank redstone 1000")
    open_menu()
    step(10)
    shot(out, 0, "progresso_10")
    step(90)
    shot(out, 1, "meio")
    step(100)
    shot(out, 2, "primeira_pronta")
    first = c.read()
    assert first["output"] == ["flint", 1] and first["tank"] == 600 and first["progress"] == 0, first
    step(50)
    c.apply("tank redstone 0")
    step(10)
    shot(out, 3, "tanque_vazio")
    dry = c.read()
    assert dry["progress"] == 50 and dry["tank"] == 0, dry
    c.apply("tank redstone 500")
    step(25)
    shot(out, 4, "quimico_volta")
    back = c.read()
    assert back["progress"] == 75 and back["tank"] == 450 and back["used_so_far"] == 75, back
    # O mundo reaberto não nasce parado: sem energia a máquina espera, e o que se salva é o que se lê depois. Sem os geradores e com a
    # energia a zero ela fica no meio da operação (progresso 75, tanque 450) até alguém pôr energia de novo.
    for generator in c.GENERATORS:
        h.command(f"setblock {c.pos(generator)} minecraft:air")
    h.command(f"data merge block {c.pos()} {{energy:0}}")
    step(3)
    waiting = c.read()
    assert waiting == back, (waiting, back)
    shot(out, 5, "antes_de_salvar")
    json.dump(back, open(STATE, "w"))
    h.ctl("quit")
    print("salvo e fechado:", back)


def depois(out):
    saved = json.load(open(STATE))
    h.ctl("options-set", "guiScale", "3")
    h.command("tick freeze")
    h.ctl("tp", "70", "-60", "96")
    h.ctl("wait", "10")
    time.sleep(1.0)
    got = c.read()
    print("reaberto:", got)
    assert got == saved, (got, saved)
    open_menu()
    shot(out, 6, "reaberto")
    h.command(f"data merge block {c.pos()} {{energy:1000}}")
    c.step(124)
    end = c.read()
    while end["output"] != ["flint", 2]:
        c.step(1)
        end = c.read()
    shot(out, 7, "segunda_pronta")
    print("fim:", end)
    assert end["output"] == ["flint", 2] and end["tank"] == 200 and end["progress"] == 0 and end["used_so_far"] == 0, end
    h.ctl("close")
    h.command("tick unfreeze")
    print("OK   ciclo completo e reabrir")


def montar(frames, saida):
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
    if modo in ("antes", "depois"):
        out = sys.argv[2] if len(sys.argv) > 2 else SHOTS
        (antes if modo == "antes" else depois)(out)
    elif modo == "montar" and len(sys.argv) == 4:
        montar(sys.argv[2], sys.argv[3])
    else:
        raise SystemExit(__doc__)
