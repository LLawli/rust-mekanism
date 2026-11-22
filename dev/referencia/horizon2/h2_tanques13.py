#!/usr/bin/env python3
"""Tanques, 13.ª rodada: o vídeo com som da ordenha com dois clientes no port (container `horizon`, os dois clientes de `abrir_dois.sh`, o anfitrião com
`ANF_EXTRA="--name Dev"`, o gamescope com `FPS=30` e `JOGO_PW_SOCK`).

Uso: H_NAME=horizon h2_tanques13.py video SEGUNDOS SAIDA.mp4
Num chão de vidro no alto (y = 120, com céu, ao meio-dia e sem chuva): o convidado ordenha a vaca dele (a posse e o leite), o anfitrião olha para ele e depois
ordenha a dele, despeja o leite num Fluid Tank e abre a tela de outro tanque com o item vazio no modo balde. A gravação é a do `gravar-jogo` (o nó do gamescope do
anfitrião, o primeiro a abrir), com o som do container num sink nulo próprio; o volume só sobe com o `seguro` dando `ok`, e volta a 0 se ele falhar."""
import os, subprocess, sys, threading, time

TOOLS = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, TOOLS)
os.environ.setdefault("H_NAME", "horizon")
import h2_dois as d

C = os.environ["H_NAME"]
AUDIO = "~/.cache/rust-mekanism/tanques-13/referencia/audio_nulo.sh"
Y = 120
TANK = "mekanism:basic_fluid_tank"
SONS = {"master": "1.0", "neutral": "1.0", "player": "1.0", "block": "1.0", "ui": "1.0", "music": "0.0", "ambient": "0.0", "weather": "0.0", "record": "0.0"}


def ssh(cmd):
    return subprocess.run(["ssh", "desktop", cmd], capture_output=True, text=True).stdout


def cmd(text):
    return d.host("command", text)


def volume(on):
    for who in (d.host, d.guest):
        for cat, val in SONS.items():
            who("options-set", f"soundCategory_{cat}", val if on else "0.0")


def vigia(parar):
    # O som só vai para os sinks de teste: a cada 0,5 s o `seguro`; se falhar, o volume mestre volta a 0 nos dois clientes.
    while not parar.is_set():
        if not ssh(f"bash {AUDIO} seguro {C} 2>&1 | tail -1").startswith("ok"):
            for who in (d.host, d.guest):
                who("options-set", "soundCategory_master", "0.0")
            print("seguro falhou: volume em 0", flush=True)
            return
        time.sleep(0.5)


def montar():
    for line in ("time set noon", "weather clear", "gamerule advance_time false", "gamerule advance_weather false", "gamemode survival @a",
                 "difficulty peaceful", "kill @e[type=minecraft:cow]", "clear @a"):
        cmd(line)
    cmd(f"fill 3 {Y} -6 15 {Y + 3} 10 minecraft:air")
    cmd(f"fill 3 {Y - 1} -2 15 {Y - 1} 8 minecraft:glass")
    cmd(f"summon minecraft:cow 7.5 {Y} 5.5 {{NoAI:1b,Rotation:[180f,0f]}}")
    cmd(f"summon minecraft:cow 11.5 {Y} 5.5 {{NoAI:1b,Rotation:[180f,0f]}}")
    cmd(f"setblock 12 {Y} 1 {TANK}")
    cmd(f"setblock 10 {Y} 1 {TANK}")
    cmd(f"tp Convidado 7.5 {Y} 3.5 0 20")
    cmd(f"tp Dev 11.5 {Y} 2.5 90 10")
    cmd(f"item replace entity Convidado weapon.mainhand with {TANK}[mekanism:bucket_mode=true]")
    cmd(f"item replace entity Dev weapon.mainhand with {TANK}[mekanism:bucket_mode=true]")


def desmontar():
    cmd(f"fill 3 {Y - 1} -6 15 {Y + 3} 10 minecraft:air")
    for line in ("kill @e[type=minecraft:cow]", "kill @e[type=minecraft:item]", "clear @a", "gamerule advance_time true", "gamerule advance_weather true"):
        cmd(line)


def cow_id(who, x):
    import json
    lista = json.loads(who("entities", "10", "cow")).get("entities", [])
    return str(min(lista, key=lambda e: abs(e["pos"][0] - x))["id"])


def video(segundos, saida):
    montar()
    time.sleep(3)
    rec = subprocess.Popen(["ssh", "desktop", f"~/.cache/rust-mekanism/gravar-jogo rec tanques-13 {saida} {segundos} {C}"])
    # O `rec` move os streams para o sink próprio antes de gravar: o volume só sobe depois de o `seguro` dar `ok`.
    time.sleep(4)
    if not ssh(f"bash {AUDIO} seguro {C} 2>&1 | tail -1").startswith("ok"):
        raise SystemExit("seguro sem ok: o volume fica em 0")
    volume(True)
    parar = threading.Event()
    threading.Thread(target=vigia, args=(parar,), daemon=True).start()
    try:
        # O anfitrião olha o convidado, que ordenha a vaca dele: a posse e o leite.
        time.sleep(2)
        d.guest("look-at", "entity", cow_id(d.guest, 7.5))
        time.sleep(1.5)
        for _ in range(2):
            d.guest("use-item")
            time.sleep(2.5)
        # O anfitrião ordenha a dele: a posse, e duas vezes o leite.
        d.host("look-at", "entity", cow_id(d.host, 11.5))
        time.sleep(2)
        for _ in range(3):
            d.host("use-item")
            time.sleep(2.5)
        # O leite vai para o Fluid Tank da frente.
        d.host("look-at", "12.5", str(Y + 0.5), "1.9")
        time.sleep(1.5)
        d.host("use-item")
        time.sleep(2.5)
        # Com o item vazio no modo balde, a tela do outro tanque abre.
        cmd(f"item replace entity Dev weapon.mainhand with {TANK}[mekanism:bucket_mode=true]")
        d.host("look-at", "10.5", str(Y + 0.5), "1.9")
        time.sleep(1.5)
        d.host("use-item")
        time.sleep(4)
        d.host("close")
        rec.wait()
    finally:
        parar.set()
        volume(False)
        if rec.poll() is None:
            rec.wait()
        desmontar()


if __name__ == "__main__":
    if sys.argv[1] == "video":
        video(int(sys.argv[2]), sys.argv[3])
