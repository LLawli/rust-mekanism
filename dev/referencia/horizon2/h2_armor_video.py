#!/usr/bin/env python3
"""Grava o ciclo completo da barra de armadura do Heat Generator, no container `horizon` (H_NAME=horizon): um print por passo (o `horizon-ctl screenshot` dá cerca de
6 quadros por segundo, pouco para um vídeo corrido, e o estado só muda no clique), com a legenda do jogo ligada (o som de equipar aparece como "Iron armor clanks") e o
HUD de armadura entre os passos, e monta o mp4 com cada passo parado por alguns segundos e uma legenda.

    H_NAME=horizon h2_armor_video.py capturar [DIRETORIO]   no desktop, com o jogo aberto: escreve passo_NN.png (um caminho que o container e o desktop enxerguem igual)
    h2_armor_video.py montar DIRETORIO SAIDA.mp4             onde houver `ffmpeg` (com `drawtext`): legenda cada passo e junta

O ciclo: abrir a tela; capacete na cabeça; capacete nas botas (recusa); o HUD com o capacete; shift-clique do peitoral do inventário (vai ao slot dele); o HUD com os dois;
shift-clique de volta (vai à barra); o HUD de novo com só o capacete; terra na mão; clique numa aba com a terra no cursor (não solta).
"""
import os, subprocess, sys, time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

STEPS = [
    ("abrir", "1. Abre o Heat Generator: a coluna de armadura e a mão, à esquerda"),
    ("capacete_na_cabeca", "2. Capacete de ferro na cabeça: aceita, e toca o som de equipar"),
    ("capacete_nas_botas", "3. Outro capacete nas botas: recusa, e a peça fica no cursor"),
    ("hud_capacete", "HUD de armadura com o capacete: 1 ícone"),
    ("shift_peitoral_entra", "4. Shift-clique do peitoral no inventário: vai ao slot do peitoral, sem som"),
    ("hud_capacete_e_peitoral", "HUD com o capacete e o peitoral: 4 ícones"),
    ("shift_peitoral_sai", "5. Shift-clique de volta: vai à barra de atalhos, sem som"),
    ("hud_so_capacete", "HUD de novo só com o capacete: 1 ícone"),
    ("terra_na_mao", "6. Terra na mão secundária: aceita"),
    ("clique_na_aba", "7. Clique na aba de energia com terra no cursor: o item não solta"),
]
HOLD_SECONDS = 2.6


def capture(out):
    import h2_armor as a
    a.ctl("options-set", "guiScale", "3")  # 640 por 360 pixels de GUI numa janela de 1920 por 1080: o canto da tela em (232, 97), os `LEFT` e `TOP` de h2_armor
    n = [0]

    def shot(name):
        time.sleep(0.5)
        a.ctl("screenshot", f"{out}/passo_{n[0]:02d}_{name}.png")
        n[0] += 1

    def reopen():
        for _ in range(4):
            a.ctl("interact", *map(str, a.SPOT))
            a.settle(0.7)
            if '"screen":"container"' in a.ctl("screen").replace(" ", ""):
                return
        raise SystemExit("o menu não abriu")

    a.ctl("options-set", "showSubtitles", "true")
    a.ctl("options-set", "chatVisibility", "2")
    a.prepare("survival")
    a.command("tick unfreeze")
    a.ctl("select", "6")  # a mão vazia: as peças que vão para a barra de atalhos não aparecem na tela
    a.command("item replace entity @s inventory.0 with minecraft:iron_chestplate 1")  # o slot 9 do menu
    a.give_hotbar(0, "minecraft:iron_helmet")
    a.give_hotbar(2, "minecraft:iron_helmet")
    a.give_hotbar(3, "minecraft:dirt", 5)
    a.give_hotbar(4, "minecraft:dirt", 5)
    time.sleep(4.5)  # as legendas do `give` somem
    ctl_ok = a.ctl("screen")
    assert '"screen":"container"' in ctl_ok.replace(" ", ""), "a tela fechou"
    shot("abrir")
    a.ctl("click-slot", "0")  # o capacete para o cursor
    a.ctl("click-slot", str(a.HEAD))
    shot("capacete_na_cabeca")
    a.ctl("click-slot", "2")
    a.ctl("click-slot", str(a.FEET))
    shot("capacete_nas_botas")
    a.ctl("click-slot", "2")  # devolve o capacete ao slot de onde veio
    a.ctl("close")
    time.sleep(3.5)
    shot("hud_capacete")
    reopen()
    a.ctl("click-slot", "9", "shift")
    shot("shift_peitoral_entra")
    a.ctl("close")
    time.sleep(3.5)
    shot("hud_capacete_e_peitoral")
    reopen()
    a.ctl("click-slot", str(a.CHEST), "shift")
    shot("shift_peitoral_sai")
    a.ctl("close")
    time.sleep(3.5)
    shot("hud_so_capacete")
    reopen()
    a.ctl("click-slot", "3")
    a.ctl("click-slot", str(a.OFFHAND))
    shot("terra_na_mao")
    a.ctl("click-slot", "4")
    x, y = a.LEFT - 13, a.TOP + 137 + 13
    a.ctl("click-at", str(x), str(y))
    time.sleep(0.4)
    shot("clique_na_aba")
    print("cursor depois do clique na aba:", a.carried(), flush=True)
    a.ctl("click-slot", "4")
    a.ctl("close")
    a.ctl("options-set", "showSubtitles", "false")
    a.command("gamemode creative")


def montar(frames, saida):
    arquivos = sorted(f for f in os.listdir(frames) if f.startswith("passo_") and f.endswith(".png"))
    if len(arquivos) != len(STEPS):
        raise SystemExit(f"{len(arquivos)} passos em {frames}, esperava {len(STEPS)}")
    fonte = "/usr/share/fonts/noto/NotoSans-Regular.ttf"
    legendadas = []
    for arquivo, (_, texto) in zip(arquivos, STEPS):
        destino = os.path.join(frames, "leg_" + arquivo)
        # A legenda numa faixa escura em cima, onde a tela não desenha nada. O texto vai por arquivo: vírgula e dois pontos quebrariam o filtro.
        arquivo_texto = os.path.join(frames, "leg_" + arquivo + ".txt")
        with open(arquivo_texto, "w") as f:
            f.write(texto)
        filtro = f"drawbox=x=0:y=0:w=iw:h=70:color=black@0.75:t=fill,drawtext=fontfile={fonte}:textfile={arquivo_texto}:fontcolor=white:fontsize=34:x=(w-text_w)/2:y=17"
        subprocess.run(["ffmpeg", "-v", "error", "-y", "-i", os.path.join(frames, arquivo), "-vf", filtro, destino], check=True)
        legendadas.append(destino)
    lista = os.path.join(frames, "lista.txt")
    with open(lista, "w") as f:
        for destino in legendadas:
            f.write(f"file '{destino}'\nduration {HOLD_SECONDS}\n")
        f.write(f"file '{legendadas[-1]}'\n")
    subprocess.run(["ffmpeg", "-v", "error", "-y", "-f", "concat", "-safe", "0", "-i", lista, "-vf", "fps=30,format=yuv420p", "-c:v", "libx264", "-crf", "22", "-movflags", "+faststart", saida], check=True)


if __name__ == "__main__":
    modo = sys.argv[1] if len(sys.argv) > 1 else ""
    if modo == "capturar":
        import h2_armor as a
        capture(sys.argv[2] if len(sys.argv) > 2 else a.SHOTS)
    elif modo == "montar" and len(sys.argv) == 4:
        montar(sys.argv[2], sys.argv[3])
    else:
        raise SystemExit(__doc__)
