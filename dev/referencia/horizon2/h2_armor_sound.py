#!/usr/bin/env python3
"""Mede, no container `horizon` (H_NAME=horizon), o som de equipar na barra de armadura do Heat Generator, pelas legendas do jogo.

O jogo sem tela não expõe o som, mas com `showSubtitles` ligado cada som audível vira uma linha na legenda (`subtitles.item.armor.equip_*`: "Leather armor
rustles", "Iron armor clanks", "Elytra rustle", "Gear equips"), que o `horizon-ctl screenshot` mostra. Cada ação espera as legendas anteriores sumirem (4 s), tira
um print antes e outro 0,4 s depois, e a região das legendas (canto de baixo à direita) é medida: texto branco ali é som, e o print fica para ler qual.

Uso: H_NAME=horizon h2_armor_sound.py [DIRETORIO_DE_PRINTS]   (um caminho que o container e o desktop enxerguem igual; o padrão é o de h2_armor.SHOTS)
Cada caso diz o que o original faz (`HeatArmorTraceTest`: `setByPlayer` chama `player.onEquipItem`, que toca o `Equippable.equipSound`, e `quickMoveStack` não).
"""
import os, subprocess, sys, time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import h2_armor as a

OUT = sys.argv[1] if len(sys.argv) > 1 else a.SHOTS
# A janela de 1920 por 1080: as legendas empilham no canto de baixo à direita, acima da barra de itens.
REGION = (1380, 640, 540, 360)
WAIT_QUIET = 4.0
CHANGE = 12  # a soma das diferenças por canal que conta como pixel mudado
MIN_TEXT = 200  # o menor texto de legenda ("Gear equips") muda uns mil pixels; sem som, nenhum
WAIT_AFTER = 0.4


def region(path):
    """A região das legendas do print, em bruto (RGB): o `ffmpeg` do desktop recorta e devolve."""
    x, y, w, h = REGION
    return subprocess.run(["ssh", "desktop", f"ffmpeg -v error -i {path} -vf crop={w}:{h}:{x}:{y} -f rawvideo -pix_fmt rgb24 -"], capture_output=True).stdout


def changed_pixels(before, after):
    """Quantos pixels da região mudaram de um print para o outro. Com o servidor parado (`tick freeze`) o mundo atrás da tela não muda, e o que muda ali é a legenda
    (conferido: sem som, 0 pixels; com "Leather armor rustles", 1998)."""
    a, b = region(before), region(after)
    return sum(1 for i in range(0, min(len(a), len(b)) - 2, 3) if abs(a[i] - b[i]) + abs(a[i + 1] - b[i + 1]) + abs(a[i + 2] - b[i + 2]) > CHANGE)


def measure(name, action, expect_sound, failures):
    """Faz `action`, e confere se a legenda apareceu (`expect_sound` True) ou não."""
    time.sleep(WAIT_QUIET)
    before, after = f"{OUT}/som_{name}_antes.png", f"{OUT}/som_{name}.png"
    a.ctl("screenshot", before)
    action()
    time.sleep(WAIT_AFTER)
    a.ctl("screenshot", after)
    changed = changed_pixels(before, after)
    a.expect(f"{name}: legenda " + ("aparece" if expect_sound else "não aparece") + f" ({changed} pixels mudaram na região)", changed >= MIN_TEXT, expect_sound, failures)


def main():
    failures = []
    a.ctl("options-set", "guiScale", "3")  # 640 por 360 pixels de GUI numa janela de 1920 por 1080, o que a região das legendas supõe
    a.ctl("options-set", "showSubtitles", "true")
    a.ctl("options-set", "chatVisibility", "2")
    a.prepare("survival")
    a.command("tick freeze")  # o mundo atrás da tela parado: só a legenda muda de um print para outro
    a.ctl("select", "6")  # a barra vazia na mão: o shift-clique para a barra cai no slot 0, e uma peça ali apareceria na mão (e seu nome na tela) dentro da região

    def hold(slot_number, item, count=1):
        a.give_hotbar(slot_number, item, count)
        a.pick(slot_number)

    def click(slot_number, *extra):
        return lambda: a.ctl("click-slot", str(slot_number), *extra)

    # Clique com o cursor: `setByPlayer` -> `onEquipItem`, o som do `Equippable` da peça.
    hold(8, "minecraft:leather_helmet")
    measure("clique_capacete_de_couro", click(a.HEAD), True, failures)
    a.expect("capacete de couro na cabeça", a.slot(a.HEAD)[0], "leather_helmet", failures)
    measure("tirar_capacete_de_couro", click(a.HEAD), False, failures)
    a.ctl("click-slot", "8")
    time.sleep(0.3)
    hold(8, "minecraft:iron_helmet")
    measure("clique_capacete_de_ferro", click(a.HEAD), True, failures)
    measure("tirar_capacete_de_ferro", click(a.HEAD), False, failures)
    a.ctl("click-slot", "8")
    time.sleep(0.3)
    hold(8, "minecraft:elytra")
    measure("clique_elitro", click(a.CHEST), True, failures)
    measure("tirar_elitro", click(a.CHEST), False, failures)
    a.ctl("click-slot", "8")
    time.sleep(0.3)
    hold(8, "minecraft:shield")
    measure("clique_escudo_na_mao", click(a.OFFHAND), True, failures)
    measure("tirar_escudo_da_mao", click(a.OFFHAND), False, failures)
    a.ctl("click-slot", "8")
    time.sleep(0.3)
    # A mão com uma peça de armadura: sem som (a mão só toca o som do que tem `Equippable` para ela).
    hold(8, "minecraft:iron_helmet")
    measure("clique_capacete_na_mao", click(a.OFFHAND), False, failures)
    a.ctl("click-slot", str(a.OFFHAND))
    time.sleep(0.3)
    a.ctl("click-slot", "8")
    time.sleep(0.3)
    hold(8, "minecraft:dirt", 5)
    measure("clique_terra_na_mao", click(a.OFFHAND), False, failures)
    a.ctl("click-slot", str(a.OFFHAND))
    time.sleep(0.3)
    a.ctl("click-slot", "8")
    time.sleep(0.3)
    # Shift-clique: `quickMoveStack` move por `insert` e `remove`, sem `setByPlayer`: sem som, nem dois ticks depois.
    a.give_hotbar(8, "minecraft:iron_helmet")
    measure("shift_capacete_para_a_armadura", click(8, "shift"), False, failures)
    a.expect("o capacete foi para a cabeça", a.slot(a.HEAD)[0], "iron_helmet", failures)
    measure("shift_da_armadura_para_a_barra", click(a.HEAD, "shift"), False, failures)
    a.expect("o capacete voltou à barra", a.slot(0)[0], "iron_helmet", failures)
    a.give_hotbar(7, "minecraft:leather_chestplate")
    measure("shift_peitoral_de_couro_para_a_armadura", click(7, "shift"), False, failures)
    a.expect("o peitoral foi para o slot dele", a.slot(a.CHEST)[0], "leather_chestplate", failures)
    # Controle: o mesmo peitoral tirado e posto pelo clique toca, para o print provar que a legenda aparece neste estado.
    a.ctl("click-slot", str(a.CHEST))
    time.sleep(0.3)
    measure("controle_clique_peitoral_de_couro", click(a.CHEST), True, failures)

    a.ctl("click-slot", str(a.CHEST))
    a.ctl("click-slot", "7")
    a.ctl("options-set", "showSubtitles", "false")
    a.command("tick unfreeze")
    a.ctl("close")
    a.command("gamemode creative")
    print(f"{len(failures)} diferenças", flush=True)
    sys.exit(1 if failures else 0)


if __name__ == "__main__":
    main()
