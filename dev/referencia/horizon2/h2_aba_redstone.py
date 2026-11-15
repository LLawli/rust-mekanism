#!/usr/bin/env python3
"""A aba de redstone em jogo (frente `aba-redstone`, `H_NAME=horizon`, mundo plano de teste): um Heat Generator aceso, com o som dele, que a aba de controle de redstone para e liga conforme o modo e uma alavanca ao lado.

    NOME=<nome do gravar-jogo start> [SEGUNDOS=40] [ARQUIVO=aba_redstone.mp4] h2_aba_redstone.py        grava o vídeo e confere cada passo (a saída diz `ok` ou `FALHA`)
    h2_aba_redstone.py                                                                                    só confere, sem gravar
    h2_aba_redstone.py item                                                                               o modo guardado no item ao quebrar e devolvido ao pôr

O tanque de lava é reposto a cada 0,15 s (o gerador queima um balde em 10 ticks). O clique é o do botão esquerdo (`NEXT_REDSTONE_CONTROL`) e o do direito (`PREVIOUS_REDSTONE_CONTROL`) no
centro do botão da aba, em (188, 150). O som do bloco é a única categoria ligada, e só depois de o stream do container estar no sink nulo (como `h2_slot_video.py`)."""
import os
import subprocess
import sys
import threading
import time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), ".."))
import h2_identity as h
import h2_nota_instrumento as n

POS = (40, -60, 40)
LEVER = (41, -60, 40)
BLOCK = "mekanismgenerators:heat_generator"
ESCALA = 3
BOTAO = (188, 150)
falhas = []


def lava():
    return 'machinery_basis:{v:1,tanks:[{resource:"minecraft:lava",amount:1000L}]}'


def tanque():
    # O buffer de energia vazio: o gerador cheio não queima (não há onde pôr a energia), e a aba não teria o que parar.
    h.command(f"data merge block {POS[0]} {POS[1]} {POS[2]} {{energy:0,{lava()}}}")


def aceso():
    return "passed" in " ".join(h.command(f"execute if block {POS[0]} {POS[1]} {POS[2]} {BLOCK}[lit=true]")).lower()


def modo():
    texto = " ".join(h.command(f"data get block {POS[0]} {POS[1]} {POS[2]} control_type"))
    for nome in ("disabled", "high", "low", "pulse"):
        if nome in texto:
            return nome
    return texto


def confere(nome, obtido, esperado):
    ok = obtido == esperado
    print(("ok    " if ok else "FALHA ") + f"{nome}: {obtido!r}" + ("" if ok else f" (esperado {esperado!r})"), flush=True)
    if not ok:
        falhas.append(nome)


def tela():
    gui_l, gui_a = -(-1920 // ESCALA), -(-1080 // ESCALA)
    return (gui_l - 176) // 2, (gui_a - 166) // 2


def sobre(ponto=BOTAO):
    esquerda, topo = tela()
    h.ctl("scroll-at", str(esquerda + ponto[0]), str(topo + ponto[1]), "0")


def clica(direito=False):
    esquerda, topo = tela()
    h.ctl("click-at", str(esquerda + BOTAO[0]), str(topo + BOTAO[1]), *(["right"] if direito else []))


def monta():
    n.sky()
    h.command("tick unfreeze")
    x, y, z = POS
    h.command(f"fill {x - 3} {y} {z - 6} {x + 3} {y + 4} {z + 3} minecraft:air")
    h.ctl("tp", f"{x}.0", f"{y}.0", f"{z - 3}.0")
    h.command("clear @s")
    h.ctl("give", BLOCK, "1")
    saida = h.ctl("place", str(x), str(y), str(z), "--item", BLOCK)
    if '"error"' in saida:
        raise SystemExit(f"place: {saida}")
    h.command("clear @s")
    h.ctl("wait", "4")
    tanque()
    h.ctl("tp", f"{x}.0", f"{y}.0", f"{z - 3}.0")
    h.ctl("look", "0", "10")
    h.ctl("options-set", "guiScale", str(ESCALA))
    h.ctl("options-set", "chatVisibility", "2")
    h.ctl("options-set", "showSubtitles", "false")


def roteiro(pausa):
    x, y, z = POS
    h.ctl("wait", "10")
    confere("modo inicial", modo(), "disabled")
    confere("aceso sem sinal no modo ignorado", aceso(), True)
    pausa(2.5)
    h.ctl("interact", str(x), str(y), str(z))
    pausa(1.0)
    sobre()
    pausa(2.0)
    for rotulo, direito, esperado in (("esquerdo: alto", False, "high"), ("esquerdo: baixo", False, "low"), ("direito: alto", True, "high"), ("direito: ignorado", True, "disabled"), ("esquerdo: alto", False, "high")):
        clica(direito)
        pausa(0.8)
        sobre()
        pausa(1.4 if esperado in ("high", "low") else 0.8)
        confere(f"clique {rotulo}", modo(), esperado)
    h.ctl("close")
    h.ctl("wait", "20")
    pausa(2.0)
    confere("alto sem sinal: apagado", aceso(), False)
    h.command(f"setblock {LEVER[0]} {LEVER[1]} {LEVER[2]} minecraft:lever[face=floor,facing=east,powered=true]")
    h.ctl("wait", "20")
    pausa(3.0)
    confere("alto com a alavanca ligada: aceso", aceso(), True)
    h.command(f"setblock {LEVER[0]} {LEVER[1]} {LEVER[2]} minecraft:lever[face=floor,facing=east,powered=false]")
    h.ctl("wait", "20")
    pausa(2.5)
    confere("alto com a alavanca desligada: apagado", aceso(), False)
    h.ctl("interact", str(x), str(y), str(z))
    pausa(1.0)
    sobre()
    clica()
    pausa(1.2)
    confere("clique esquerdo: baixo", modo(), "low")
    h.ctl("close")
    h.ctl("wait", "20")
    pausa(2.5)
    confere("baixo sem sinal: aceso", aceso(), True)
    h.command(f"setblock {LEVER[0]} {LEVER[1]} {LEVER[2]} minecraft:lever[face=floor,facing=east,powered=true]")
    h.ctl("wait", "20")
    pausa(2.5)
    confere("baixo com sinal: apagado", aceso(), False)
    h.command(f"setblock {LEVER[0]} {LEVER[1]} {LEVER[2]} minecraft:air")


def item():
    x, y, z = POS
    monta()
    h.ctl("interact", str(x), str(y), str(z))
    time.sleep(1.0)
    clica()
    time.sleep(1.0)
    confere("modo antes de quebrar", modo(), "high")
    h.ctl("close")
    h.command("gamemode survival")
    h.command("give @s minecraft:diamond_pickaxe")
    h.ctl("select", "minecraft:diamond_pickaxe")
    h.command("kill @e[type=minecraft:item]")
    h.ctl("tp", f"{x}.0", f"{y}.0", f"{z - 3}.0")
    h.ctl("break", str(x), str(y), str(z))
    time.sleep(1.2)
    nbt = " ".join(h.command("data get entity @e[type=minecraft:item,limit=1,sort=nearest] Item"))
    confere("o item solto leva o modo", "mekanism:redstone_control" in nbt and "high" in nbt, True)
    h.command("gamemode creative")
    h.command("kill @e[type=minecraft:item]")
    h.ctl("give", BLOCK, "1")
    h.ctl("place", str(x), str(y), str(z), "--item", BLOCK)
    time.sleep(1.0)
    confere("posto de volta (um item novo): ignorado", modo(), "disabled")
    h.command(f"setblock {x} {y} {z} minecraft:air")
    # O item que o original solta com o modo (`mekanism:redstone_control`): posto, o tile volta a ter o modo.
    h.command("clear @s")
    h.command(f'give @s {BLOCK}[mekanism:redstone_control="high"] 1')
    h.ctl("select", "1")
    h.ctl("tp", f"{x}.0", f"{y}.0", f"{z - 3}.0")
    h.ctl("select", "1")
    h.ctl("place", str(x), str(y), str(z), "--item", BLOCK)
    time.sleep(1.0)
    confere("posto de volta (o item com o modo): alto", modo(), "high")
    h.command(f"setblock {x} {y} {z} minecraft:air")
    h.command("clear @s")


def main():
    if "item" in sys.argv[1:]:
        item()
        print("falhas:", falhas)
        return
    nome = os.environ.get("NOME")
    segundos, arquivo = os.environ.get("SEGUNDOS", "40"), os.environ.get("ARQUIVO", "aba_redstone.mp4")
    monta()
    if not nome:
        roteiro(time.sleep)
        print("falhas:", falhas)
        return
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
        time.sleep(4.0)
        roteiro(time.sleep)
        out, err = rec.communicate(timeout=int(segundos) + 120)
        print(out, err, flush=True)
    finally:
        n.mute()
        parar.set()
        if rec is not None and rec.poll() is None:
            rec.wait(timeout=60)
        h.ctl("close")
        h.command(f"setblock {POS[0]} {POS[1]} {POS[2]} minecraft:air")
        h.command("clear @s")
        h.ctl("options-set", "chatVisibility", "0")
        n.restore()
    print("falhas:", falhas)


if __name__ == "__main__":
    main()
