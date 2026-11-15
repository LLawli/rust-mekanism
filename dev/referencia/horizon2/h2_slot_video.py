#!/usr/bin/env python3
"""O vídeo da frente `slot-energia`, em jogo (`H_NAME=horizon2`, mundo plano de teste): um Heat Generator aceso, com o som dele, entregando energia ao item do slot de energia
(`EnergyInventorySlot.drainContainerIntoSlot`). O jogador vê o gerador aceso por fora, abre a tela e a barra do item do slot enche; no fim o cursor para sobre o slot e o tooltip dá a energia.

    NOME=<nome do gravar-jogo start> [SEGUNDOS=34] [ARQUIVO=slot.mp4] h2_slot_video.py

O gerador queima um balde de lava em 10 ticks (100 mB por tick), por isso o tanque é reposto a cada 0,15 s (só o tanque, por `data merge`; o item e a energia do tile ficam). O som do bloco
é a única categoria ligada (o do gerador é `block`) e o volume mestre só sobe depois de o stream do container estar no sink nulo; um vigia volta o mestre a 0 se o stream sair dos sinks de teste
(`h2_nota_instrumento.video`, a mesma lógica). A gravação é a do `gravar-jogo rec <NOME> <arquivo> <s> <container>` (o container nasce com `JOGO_PW_SOCK`)."""
import os
import subprocess
import sys
import threading
import time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), ".."))
import h2_identity as h
import h2_nota_instrumento as n
import cena_telas as t

POS = (40, -60, 40)
ITEM = "mekanism:free_runners"
ESCALA = 3


def tanque():
    h.command(f'data merge block {POS[0]} {POS[1]} {POS[2]} {{{n_lava()}}}')


def n_lava():
    return 'machinery_basis:{v:1,tanks:[{resource:"minecraft:lava",amount:1000L}]}'


def main():
    nome, segundos, arquivo = os.environ["NOME"], os.environ.get("SEGUNDOS", "34"), os.environ.get("ARQUIVO", "slot.mp4")
    n.sky()
    h.command("tick unfreeze")
    x, y, z = POS
    # Uma clareira no plano, com céu: nada atrás do gerador além do chão.
    h.command(f"fill {x - 3} {y} {z - 6} {x + 3} {y + 4} {z + 3} minecraft:air")
    h.ctl("tp", f"{x}.0", f"{y}.0", f"{z - 3}.0")
    h.command("clear @s")
    h.ctl("give", "mekanismgenerators:heat_generator", "1")
    saida = h.ctl("place", str(x), str(y), str(z), "--item", "mekanismgenerators:heat_generator")
    if '"error"' in saida:
        raise SystemExit(f"place: {saida}")
    h.command("clear @s")
    h.ctl("wait", "4")
    h.command(f'data merge block {x} {y} {z} {{Items:[{{Slot:1b,id:"{ITEM}",count:1}}],{n_lava()}}}')
    h.ctl("tp", f"{x}.0", f"{y}.0", f"{z - 3}.0")
    h.ctl("look", "0", "10")
    h.ctl("options-set", "guiScale", str(ESCALA))
    h.ctl("options-set", "chatVisibility", "2")
    h.ctl("options-set", "showSubtitles", "false")
    for categoria in n.SOUND_OFF + ("record",):
        h.ctl("options-set", f"soundCategory_{categoria}", "0")
    h.ctl("options-set", "soundCategory_block", "1.0")
    n.mute()
    ligado = False
    for _ in range(60):
        if "movidos 0" not in n.desktop(f"ligar {n.CONTAINER}").stdout and n.desktop(f"verificar {n.CONTAINER}").stdout.startswith("ok"):
            ligado = True
            break
        time.sleep(1)
    if not ligado:
        print("o stream do jogo não foi para o sink nulo: nada gravado, volume mestre em 0")
        sys.exit(1)
    print("stream no sink nulo:", n.desktop(f"verificar {n.CONTAINER}").stdout.strip(), flush=True)
    parar = threading.Event()

    def vigia():
        while not parar.is_set():
            if not n.desktop(f"seguro {n.CONTAINER}").stdout.startswith("ok"):
                print("stream fora dos sinks de teste: volume mestre em 0", flush=True)
                n.mute()
            parar.wait(0.5)

    def repoe():
        while not parar.is_set():
            tanque()
            parar.wait(0.15)

    for alvo in (vigia, repoe):
        threading.Thread(target=alvo, daemon=True).start()
    rec = None
    try:
        h.ctl("options-set", "soundCategory_master", "1.0")
        time.sleep(3.0)
        rec = subprocess.Popen(["ssh", "desktop", f"{n.GRAVAR} rec {nome} {arquivo} {segundos} {n.CONTAINER}"], stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
        time.sleep(9.0)
        # O gerador aceso por fora, depois a tela com a barra do item enchendo.
        h.ctl("interact", str(x), str(y), str(z))
        time.sleep(0.5)
        h.ctl("look", "0", "0")
        time.sleep(float(segundos) - 9.0 - 0.5 - 5.0 - 1.0)
        gui_l = -(-1920 // ESCALA)
        gui_a = -(-1080 // ESCALA)
        esquerda = (gui_l - 176) // 2
        topo = (gui_a - 166) // 2
        hx, hy = t.HOVER["generator"]["slot_energia"]
        h.ctl("scroll-at", str(esquerda + hx), str(topo + hy), "0")
        time.sleep(5.0)
        out, err = rec.communicate(timeout=int(segundos) + 90)
        print(out, err, flush=True)
    finally:
        n.mute()
        parar.set()
        if rec is not None and rec.poll() is None:
            rec.wait(timeout=60)
        h.ctl("close")
        h.command(f"setblock {x} {y} {z} minecraft:air")
        h.command("clear @s")
        h.ctl("options-set", "chatVisibility", "0")
        n.restore()


if __name__ == "__main__":
    main()
